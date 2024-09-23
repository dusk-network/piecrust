// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Library for creating and managing copy-on-write memory-mapped regions.
//!
//! The core functionality is offered by the [`Mmap`] struct, which is a
//! read-write memory region that keeps track of which pages have been written
//! to.
//!
//! # Example
//! ```rust
//! # use std::io;
//! # fn main() -> io::Result<()> {
//! use crumbles::Mmap;
//!
//! let mut mmap = Mmap::new(65536, 65536)?;
//!
//! // When first created, the mmap is not dirty.
//! assert_eq!(mmap.dirty_pages().count(), 0);
//!
//! mmap[24] = 42;
//! // After writing a single byte, the page it's on is dirty.
//! assert_eq!(mmap.dirty_pages().count(), 1);
//! # Ok(())
//! # }
//! ```
//!
//! # Limitations
//! This crate currently only builds for 64-bit Unix targets. This is because it
//! relies on various features of `libc` which are not available in other
//! targets.
#![cfg(all(unix, target_pointer_width = "64"))]
#![deny(missing_docs)]
#![deny(clippy::pedantic)]

use std::{
    collections::{btree_map::Entry, BTreeMap},
    mem::{self, MaybeUninit},
    ops::{Deref, DerefMut},
    sync::{Once, OnceLock, RwLock},
    {io, process, ptr, slice},
};

use libc::{
    c_int, sigaction, sigemptyset, siginfo_t, sigset_t, ucontext_t,
    MAP_ANONYMOUS, MAP_FAILED, MAP_NORESERVE, MAP_PRIVATE, PROT_NONE,
    PROT_READ, PROT_WRITE, SA_SIGINFO,
};

/// A handle to a copy-on-write memory-mapped region that keeps track of which
/// pages have been written to.
///
/// An `Mmap` may be created totally filled with zeros using [`new`], or be
/// instantiated to use a closure to load the contents of the page using
/// [`with_pages`]. In cases the `Mmap` will only use the amount of memory
/// actually accessed during its lifetime.
///
/// It is possible to create snapshots of the memory, which can be used to
/// revert to a previous state. See [`snap`] for more details.
///
/// Writes are tracked at the page level. This functions as follows:
///
/// - When a region is first mapped, its permissions are set to none, resulting
///   in a SIGSEGV when a read or a write are attempted.
/// - When a read/write is attempted, the SIGSEGV is caught, the page contents
///   loaded and written to the appropriate location in memory.
/// - When a write is subsequently attempted, the SIGSEGV is caught using a
///   signal handler, which proceeds to set the permissions of the page to
///   read-write while also marking it as dirty.
///
/// `Mmap` is [`Sync`] and [`Send`].
///
/// [`new`]: Mmap::new
/// [`with_pages`]: Mmap::with_pages
/// [`snap`]: Mmap::snap
pub struct Mmap(&'static mut MmapInner);

impl Mmap {
    /// Create a new mmap, backed entirely by physical memory. The memory is
    /// initialized to all zeros.
    ///
    /// The size of the memory region is specified by the caller using the
    /// number of pages - `page_number` - and the page size - `page_size`. The
    /// size total size of the region will then be `page_number * page_size`.
    ///
    /// # Errors
    /// If the underlying call to map memory fails, the function will return an
    /// error.
    ///
    /// # Example
    /// ```rust
    /// # use std::io;
    /// # fn main() -> io::Result<()> {
    /// use crumbles::Mmap;
    ///
    /// let mmap = Mmap::new(65536, 65536)?;
    /// assert_eq!(mmap[..0x10_000], [0; 0x10_000]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(page_number: usize, page_size: usize) -> io::Result<Self> {
        Self::with_pages(page_number, page_size, default_load_page)
    }

    /// Create a new mmap, backed by physical memory, that will query for pages
    /// using the given `page_locator`. Conceptually the page locator is a
    /// closure, taking a page index and a page buffer as argument, that writes
    /// the existing contents of that page (if any) to the buffer.
    ///
    /// The size of the memory region is specified by the caller using the
    /// number of pages - `page_number` - and the page size - `page_size`. The
    /// size total size of the region will then be `page_number * page_size`.
    ///
    /// # Errors
    /// If the underlying call to map memory fails, the function will return an
    /// error. The pages loaded by the `page_loader` must be not fail to load,
    /// otherwise the process will halt.
    ///
    /// # Example
    /// ```rust
    /// # use std::io;
    /// # fn main() -> io::Result<()> {
    /// use std::fs::File;
    /// use std::io::Read;
    /// use std::path::PathBuf;
    ///
    /// use crumbles::Mmap;
    ///
    /// const PAGE_SIZE: usize = 65536;
    ///
    /// let mut page = [42; PAGE_SIZE];
    ///
    /// let mmap = unsafe {
    ///     Mmap::with_pages(65536, PAGE_SIZE, move |page_index, buf: &mut [u8]| {
    ///         match page_index {
    ///             0 => {
    ///                 buf.copy_from_slice(&page);
    ///                 Ok(PAGE_SIZE)
    ///             },
    ///             _ => Ok(0),
    ///         }
    ///     })?
    /// };
    /// assert_eq!(mmap[..PAGE_SIZE], page[..]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_pages<LP>(
        page_index: usize,
        page_size: usize,
        page_loader: LP,
    ) -> io::Result<Self>
    where
        LP: 'static + LoadPage,
    {
        let inner =
            unsafe { MmapInner::new(page_index, page_size, page_loader)? };

        with_global_map_mut(|global_map| {
            let inner = Box::leak(Box::new(inner));

            let start_addr = inner.bytes.as_mut_ptr() as usize;
            let end_addr = start_addr + inner.bytes.len();

            let inner_ptr = inner as *mut _;

            global_map.insert(start_addr..end_addr, inner_ptr as _);

            Ok(Self(inner))
        })
    }

    /// Snapshot the current state of the memory.
    ///
    /// Snapshotting the memory should be done when the user wants to create a
    /// point in time to which they can revert to. This is useful when they
    /// want to perform a series of operations, and either [`revert`] back to
    /// the original or [`apply`] to keep the changes.
    ///
    /// # Errors
    /// If the underlying call to protect the memory region fails, this function
    /// will error. When this happens, the memory region will be left in an
    /// inconsistent state, and the caller is encouraged to drop the structure.
    ///
    /// # Example
    /// ```rust
    /// # use std::io;
    /// # fn main() -> io::Result<()> {
    /// use crumbles::Mmap;
    ///
    /// let mut mmap = Mmap::new(65536, 65536)?;
    ///
    /// mmap[0] = 1;
    /// mmap.snap()?;
    ///
    /// // Snapshotting the memory keeps the current state, and also resets
    /// // dirty pages to clean.
    /// assert_eq!(mmap[0], 1);
    /// assert_eq!(mmap.dirty_pages().count(), 0);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [`revert`]: Mmap::revert
    /// [`apply`]: Mmap::apply
    pub fn snap(&mut self) -> io::Result<()> {
        unsafe { self.0.snap() }
    }

    /// Revert to the last snapshot.
    ///
    /// Reverting means discarding all changes made since the last snapshot was
    /// taken using [`snap`]. If no snapshot was taken, this will reset the
    /// memory to its initial state on instantiation.
    ///
    /// # Errors
    /// If the underlying call to protect the memory region fails, this function
    /// will error. When this happens, the memory region will be left in an
    /// inconsistent state, and the caller is encouraged to drop the structure.
    ///
    /// # Example
    /// ```rust
    /// # use std::io;
    /// # fn main() -> io::Result<()> {
    /// use crumbles::Mmap;
    ///
    /// let mut mmap = Mmap::new(65536, 65536)?;
    ///
    /// mmap[0] = 1;
    /// mmap.revert()?;
    ///
    /// assert_eq!(mmap[0], 0);
    /// assert_eq!(mmap.dirty_pages().count(), 0);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [`snap`]: Mmap::snap
    pub fn revert(&mut self) -> io::Result<()> {
        unsafe { self.0.revert() }
    }

    /// Apply current changes to the last snapshot.
    ///
    /// Applying the current changes means keeping them and merging them with
    /// the changes made since the last snapshot was taken using [`snap`].
    /// If no snapshot was taken, this call will have no effect.
    ///
    /// # Errors
    /// If the underlying call to protect the memory region fails, this function
    /// will error. When this happens, the memory region will be left in an
    /// inconsistent state, and the caller is encouraged to drop the structure.
    ///
    /// # Example
    /// ```rust
    /// # use std::io;
    /// # fn main() -> io::Result<()> {
    /// use crumbles::Mmap;
    ///
    /// let mut mmap = Mmap::new(65536, 65536)?;
    ///
    /// mmap[0] = 1;
    /// mmap.apply()?;
    ///
    /// assert_eq!(mmap[0], 1);
    /// assert_eq!(mmap.dirty_pages().count(), 1);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [`snap`]: Mmap::snap
    pub fn apply(&mut self) -> io::Result<()> {
        unsafe { self.0.apply() }
    }

    /// Returns an iterator over dirty memory pages and their clean
    /// counterparts, together with their offsets.
    ///
    /// # Example
    /// ```rust
    /// # use std::io;
    /// # fn main() -> io::Result<()> {
    /// use crumbles::Mmap;
    ///
    /// let mut mmap = Mmap::new(65536, 65536)?;
    /// mmap[0x10_000] = 1; // second page
    ///
    /// let dirty_pages: Vec<_> = mmap.dirty_pages().collect();
    ///
    /// assert_eq!(dirty_pages.len(), 1);
    /// assert_eq!(*dirty_pages[0].2, 1, "Index of the first page");
    /// # Ok(())
    /// # }
    /// ```
    pub fn dirty_pages(&self) -> impl Iterator<Item = (&[u8], &[u8], &usize)> {
        self.0.last_snapshot().clean_pages.iter().map(
            move |(page_index, clean_page)| {
                let page_size = self.0.page_size;
                let offset = page_index * page_size;
                (
                    &self.0.bytes[offset..][..page_size],
                    &clean_page[..],
                    page_index,
                )
            },
        )
    }
}

impl AsRef<[u8]> for Mmap {
    fn as_ref(&self) -> &[u8] {
        self.0.bytes
    }
}

impl AsMut<[u8]> for Mmap {
    fn as_mut(&mut self) -> &mut [u8] {
        self.0.bytes
    }
}

impl Deref for Mmap {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.0.bytes
    }
}

impl DerefMut for Mmap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.bytes
    }
}

// This `Drop` implementation removes the inner memory struct pointer from the
// global map.
impl Drop for Mmap {
    fn drop(&mut self) {
        with_global_map_mut(|global_map| {
            unsafe {
                let inner_ptr = self.0 as *mut MmapInner;
                let inner = Box::from_raw(inner_ptr);

                let start_addr = inner.bytes.as_mut_ptr() as usize;
                let len = inner.bytes.len();
                let end_addr = start_addr + len;

                global_map.remove(start_addr..end_addr);
            };
        });
    }
}

type InnerMap = rangemap::RangeMap<usize, usize>;

/// Global memory map. Map from the address range of a mapping to the pointer to
/// the inner memory struct that contains it.
static INNER_MAP: OnceLock<RwLock<InnerMap>> = OnceLock::new();

fn with_global_map<T, F>(closure: F) -> T
where
    F: FnOnce(&InnerMap) -> T,
{
    let global_map = INNER_MAP
        .get_or_init(|| RwLock::new(InnerMap::new()))
        .read()
        .unwrap();

    closure(&global_map)
}

fn with_global_map_mut<T, F>(closure: F) -> T
where
    F: FnOnce(&mut InnerMap) -> T,
{
    let mut global_map = INNER_MAP
        .get_or_init(|| RwLock::new(InnerMap::new()))
        .write()
        .unwrap();

    closure(&mut global_map)
}

#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
fn system_page_size() -> usize {
    static mut PAGE_SIZE: OnceLock<usize> = OnceLock::new();
    unsafe {
        *PAGE_SIZE.get_or_init(|| libc::sysconf(libc::_SC_PAGESIZE) as usize)
    }
}

/// Contains clean pages, together with a bitset of pages that have already been
/// hit at least one SIGSEGV - i.e. marked as having been read.
struct Snapshot {
    clean_pages: BTreeMap<usize, Vec<u8>>,
    hit_pages: PageBits,
}

impl Snapshot {
    fn new(page_number: usize) -> io::Result<Self> {
        Ok(Self {
            clean_pages: BTreeMap::new(),
            hit_pages: PageBits::new(page_number)?,
        })
    }
}

/// One bit for each page - in memory.
struct PageBits(&'static mut [u8]);

impl PageBits {
    /// Maps one bit per each page of memory.
    fn new(page_number: usize) -> io::Result<Self> {
        let page_bits = unsafe {
            let len = page_number / 8 + usize::from(page_number % 8 != 0);

            let ptr = libc::mmap(
                ptr::null_mut(),
                len,
                PROT_READ | PROT_WRITE,
                MAP_PRIVATE | MAP_ANONYMOUS | MAP_NORESERVE,
                -1,
                0,
            );

            if ptr == MAP_FAILED {
                return Err(io::Error::last_os_error());
            }

            slice::from_raw_parts_mut(ptr.cast(), len)
        };

        Ok(Self(page_bits))
    }

    /// Execute the given closure with `true` if the bit was set, or `false` if
    /// the bit was not set. The bit will always be set after the closure is
    /// executed successfully.
    fn set_and_exec<T, E, F>(
        &mut self,
        page_index: usize,
        closure: F,
    ) -> Result<T, E>
    where
        F: FnOnce(bool) -> Result<T, E>,
    {
        let byte_index = page_index / 8;
        let bit_index = page_index % 8;

        let byte = &mut self.0[byte_index];
        let mask = 1u8 << bit_index;

        match *byte & mask {
            0 => {
                let r = closure(false);
                if r.is_ok() {
                    *byte |= mask;
                }
                r
            }
            _ => closure(true),
        }
    }
}

impl Drop for PageBits {
    fn drop(&mut self) {
        unsafe {
            let ptr = self.0.as_mut_ptr();
            let len = self.0.len();

            libc::munmap(ptr.cast(), len);
        }
    }
}

/// Types that can used to load a page's content.
pub trait LoadPage: Send + Sync {
    /// Loads the page's contents at the given `index` and subsequently writes
    /// it to the given `buf`fer.
    ///
    /// # Errors
    /// Implementations are encouraged to error if loading a page's content
    /// fails, but should return `Ok(0)` if the page's content is not known,
    /// and not write to the buffer at all. In such a situation, the page
    /// will be filled zeros.
    fn load_page(&mut self, index: usize, buf: &mut [u8]) -> io::Result<usize>;
}

/// The default implementation of [`LoadPage`] just returns 0 and never touches
/// the buffer.
fn default_load_page(_: usize, _: &mut [u8]) -> io::Result<usize> {
    Ok(0)
}

impl<F> LoadPage for F
where
    for<'a> F: FnMut(usize, &'a mut [u8]) -> io::Result<usize>,
    F: Send + Sync,
{
    fn load_page(&mut self, index: usize, buf: &mut [u8]) -> io::Result<usize> {
        self(index, buf)
    }
}

/// Contains the actual memory region, together with the set of dirty pages.
struct MmapInner {
    bytes: &'static mut [u8],

    page_size: usize,
    page_number: usize,
    page_buf: Vec<u8>,

    mapped_pages: PageBits,
    snapshots: Vec<Snapshot>,

    page_loader: Box<dyn LoadPage>,
}

impl MmapInner {
    unsafe fn new<LP>(
        page_number: usize,
        page_size: usize,
        page_loader: LP,
    ) -> io::Result<Self>
    where
        LP: 'static + LoadPage,
    {
        setup_action();

        let system_page_size = system_page_size();
        if page_size % system_page_size != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "Page size {page_size} must be a multiple \
                     of the system page size {system_page_size}"
                ),
            ));
        }

        let mapped_pages = PageBits::new(page_number)?;
        let snapshot = Snapshot::new(page_number)?;

        let bytes = {
            let len = page_number * page_size;

            let ptr = libc::mmap(
                ptr::null_mut(),
                len,
                PROT_NONE,
                MAP_PRIVATE | MAP_ANONYMOUS | MAP_NORESERVE,
                -1,
                0,
            );

            if ptr == MAP_FAILED {
                return Err(io::Error::last_os_error());
            }

            slice::from_raw_parts_mut(ptr.cast(), len)
        };

        let page_buf = vec![0; page_size];

        Ok(Self {
            bytes,
            page_size,
            page_number,
            page_buf,
            mapped_pages,
            // There should always be at least one snapshot
            snapshots: vec![snapshot],
            page_loader: Box::new(page_loader),
        })
    }

    /// Processes a segfault at the given address. The address must be
    /// guaranteed to be within the memory region.
    ///
    /// Before segfaulting, the entire memory region is protected with
    /// `PROT_NONE`. When either a read or a write is attempted, the loaded page
    /// contents will be copied onto the accessed memory, and the permissions
    /// will then be set to `PROT_READ` for that page. If a write is attempted,
    /// a new segfault will occur, and the permissions will be set to `PROT_READ
    /// | PROT_WRITE` for that page.
    ///
    /// This is possible due to the keeping of two bits per page - one for
    /// whether the page has been mapped, and one for whether the page has
    /// been hit at least once.
    unsafe fn process_segv(&mut self, si_addr: usize) -> io::Result<()> {
        let start_addr = self.bytes.as_mut_ptr() as usize;
        let page_size = self.page_size;
        let page_index = (si_addr - start_addr) / page_size;

        let start_addr = self.bytes.as_ptr() as usize;
        let page_offset = page_index * self.page_size;

        let page_addr = start_addr + page_offset;
        let page_size = self.page_size;

        // Load the page given by the locator, if any, at the given offset.
        // If we've already loaded it, we don't need to do so again.
        self.mapped_pages.set_and_exec(
            page_index,
            |is_bit_set| -> io::Result<()> {
                if is_bit_set {
                    return Ok(());
                }

                ptr::write_bytes(self.page_buf.as_mut_ptr(), 0, self.page_size);
                self.page_loader.load_page(page_index, &mut self.page_buf)?;

                let prot = PROT_READ | PROT_WRITE;
                if libc::mprotect(page_addr as _, page_size, prot) != 0 {
                    return Err(io::Error::last_os_error());
                }

                self.bytes[page_offset..][..page_size]
                    .copy_from_slice(&self.page_buf);

                let prot = PROT_NONE;
                if libc::mprotect(page_addr as _, page_size, prot) != 0 {
                    return Err(io::Error::last_os_error());
                }

                Ok(())
            },
        )?;

        let snapshot = self
            .snapshots
            .last_mut()
            .expect("There should always be at least one snapshot");

        // If the page wasn't hit before, set read only permissions for the
        // page. If it was set before, we're writing and need to set read-write
        // permissions, and mark the page as dirty.
        snapshot.hit_pages.set_and_exec(page_index, |is_bit_set| {
            let mut prot = PROT_READ;

            if is_bit_set {
                prot |= PROT_WRITE;

                if let Entry::Vacant(e) = snapshot.clean_pages.entry(page_index)
                {
                    let mut clean_page = vec![0; page_size];
                    clean_page.copy_from_slice(
                        &self.bytes[page_offset..][..page_size],
                    );
                    e.insert(clean_page);
                }
            }

            if libc::mprotect(page_addr as _, page_size, prot) != 0 {
                return Err(io::Error::last_os_error());
            }

            Ok(())
        })?;

        Ok(())
    }

    unsafe fn snap(&mut self) -> io::Result<()> {
        let len = self.bytes.len();

        if libc::mprotect(self.bytes.as_mut_ptr().cast(), len, PROT_NONE) != 0 {
            return Err(io::Error::last_os_error());
        }

        self.snapshots.push(Snapshot::new(self.page_number)?);

        Ok(())
    }

    unsafe fn apply(&mut self) -> io::Result<()> {
        let len = self.bytes.len();

        if libc::mprotect(self.bytes.as_mut_ptr().cast(), len, PROT_NONE) != 0 {
            return Err(io::Error::last_os_error());
        }

        let popped_snapshot = self
            .snapshots
            .pop()
            .expect("There should always be at least one snapshot");
        if self.snapshots.is_empty() {
            self.snapshots.push(Snapshot::new(self.page_number)?);
        }
        let snapshot = self.last_snapshot_mut();

        for (page_index, clean_page) in popped_snapshot.clean_pages {
            snapshot.clean_pages.entry(page_index).or_insert(clean_page);
        }

        Ok(())
    }

    unsafe fn revert(&mut self) -> io::Result<()> {
        let popped_snapshot = self
            .snapshots
            .pop()
            .expect("There should always be at least one snapshot");

        if self.snapshots.is_empty() {
            self.snapshots.push(Snapshot::new(self.page_number)?);
        } else {
            self.last_snapshot_mut().hit_pages =
                PageBits::new(self.page_number)?;
        }

        let page_size = self.page_size;

        for (page_index, clean_page) in popped_snapshot.clean_pages {
            let page_offset = page_index * page_size;
            self.bytes[page_offset..][..page_size]
                .copy_from_slice(&clean_page[..]);
        }

        let len = self.bytes.len();

        if libc::mprotect(self.bytes.as_mut_ptr().cast(), len, PROT_NONE) != 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(())
    }

    fn last_snapshot(&self) -> &Snapshot {
        self.snapshots
            .last()
            .expect("There should always be at least one snapshot")
    }

    fn last_snapshot_mut(&mut self) -> &mut Snapshot {
        self.snapshots
            .last_mut()
            .expect("There should always be at least one snapshot")
    }
}

impl Drop for MmapInner {
    fn drop(&mut self) {
        unsafe {
            let ptr = self.bytes.as_mut_ptr();
            let len = self.bytes.len();

            libc::munmap(ptr.cast(), len);
        }
    }
}

static SIGNAL_HANDLER: Once = Once::new();

// Sets up [`segfault_handler`] to handle SIGSEGV, and returns the previous
// action used to handle it, if any.
unsafe fn setup_action() -> sigaction {
    static OLD_ACTION: OnceLock<sigaction> = OnceLock::new();

    SIGNAL_HANDLER.call_once(|| {
        let mut sa_mask = MaybeUninit::<sigset_t>::uninit();
        sigemptyset(sa_mask.as_mut_ptr());

        let act = sigaction {
            sa_sigaction: segfault_handler as _,
            sa_mask: sa_mask.assume_init(),
            sa_flags: SA_SIGINFO,
            #[cfg(target_os = "linux")]
            sa_restorer: None,
        };
        let mut old_act = MaybeUninit::<sigaction>::uninit();

        if libc::sigaction(libc::SIGSEGV, &act, old_act.as_mut_ptr()) != 0 {
            process::exit(1);
        }

        // On Apple Silicon for some reason SIGBUS is thrown instead of SIGSEGV.
        // TODO should investigate properly
        #[cfg(target_os = "macos")]
        if libc::sigaction(libc::SIGBUS, &act, old_act.as_mut_ptr()) != 0 {
            process::exit(2);
        }

        OLD_ACTION.get_or_init(move || old_act.assume_init());
    });

    *OLD_ACTION.get().unwrap()
}

/// Calls the old action that was set to handle `SIGSEGV`
unsafe fn call_old_action(
    sig: c_int,
    info: *mut siginfo_t,
    ctx: *mut ucontext_t,
) {
    let old_act = setup_action();

    // If SA_SIGINFO is set, the old action is a `fn(c_int, *mut siginfo_t, *mut
    // ucontext_t)`. Otherwise, it's a `fn(c_int)`.
    if old_act.sa_flags & SA_SIGINFO == 0 {
        let act: fn(c_int) = mem::transmute(old_act.sa_sigaction);
        act(sig);
    } else {
        let act: fn(c_int, *mut siginfo_t, *mut ucontext_t) =
            mem::transmute(old_act.sa_sigaction);
        act(sig, info, ctx);
    }
}

unsafe fn segfault_handler(
    sig: c_int,
    info: *mut siginfo_t,
    ctx: *mut ucontext_t,
) {
    with_global_map(move |global_map| {
        let si_addr = (*info).si_addr() as usize;

        if let Some(inner_ptr) = global_map.get(&si_addr) {
            let inner = &mut *(*inner_ptr as *mut MmapInner);

            if inner.process_segv(si_addr).is_err() {
                call_old_action(sig, info, ctx);
            }

            return;
        }

        call_old_action(sig, info, ctx);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};
    use std::thread;

    const N_PAGES: usize = 65536;
    const PAGE_SIZE: usize = 65536;

    const DIRT: [u8; 2 * PAGE_SIZE] = [42; 2 * PAGE_SIZE];
    const DIRT2: [u8; 2 * PAGE_SIZE] = [43; 2 * PAGE_SIZE];

    const OFFSET: usize = PAGE_SIZE / 2 + PAGE_SIZE;

    #[test]
    fn write() {
        let mut mem = Mmap::new(N_PAGES, PAGE_SIZE)
            .expect("Instantiating new memory should succeed");

        let slice = &mut mem[OFFSET..][..DIRT.len()];
        slice.copy_from_slice(&DIRT);

        assert_eq!(slice, DIRT, "Slice should be dirt just written");
        assert_eq!(mem.dirty_pages().count(), 3);
    }

    #[test]
    fn write_multi_thread() {
        const NUM_THREADS: usize = 8;

        let mut threads = Vec::with_capacity(NUM_THREADS);

        for _ in 0..NUM_THREADS {
            threads.push(thread::spawn(|| {
                let mut mem = Mmap::new(N_PAGES, PAGE_SIZE)
                    .expect("Instantiating new memory should succeed");

                let slice = &mut mem[OFFSET..][..DIRT.len()];
                slice.copy_from_slice(&DIRT);

                assert_eq!(slice, DIRT, "Slice should be dirt just written");
                assert_eq!(mem.dirty_pages().count(), 3);
            }));
        }

        threads
            .drain(..)
            .for_each(|t| t.join().expect("Thread should exit cleanly"));
    }

    #[test]
    fn revert() {
        let mut mem = Mmap::new(N_PAGES, PAGE_SIZE)
            .expect("Instantiating new memory should succeed");

        let slice = &mut mem[OFFSET..][..DIRT.len()];
        slice.copy_from_slice(&DIRT);

        mem.snap().expect("Snapshotting should succeed");

        assert_eq!(mem.dirty_pages().count(), 0);
        let slice = &mem[OFFSET..][..DIRT.len()];
        assert_eq!(slice, DIRT, "Slice should be dirt just written");

        // Writing to the same page should now be reversible
        let slice = &mut mem[OFFSET..][..DIRT.len()];
        slice.copy_from_slice(&[0; 2 * PAGE_SIZE]);

        mem.revert().expect("Reverting should succeed");

        assert_eq!(mem.dirty_pages().count(), 3);
        let slice = &mut mem[OFFSET..][..DIRT.len()];
        assert_eq!(
            slice, DIRT,
            "Slice should be the dirt that was written before"
        );
    }

    #[test]
    fn multi_revert() {
        let mut mem = Mmap::new(N_PAGES, PAGE_SIZE)
            .expect("Instantiating new memory should succeed");

        let slice = &mut mem[OFFSET..][..DIRT.len()];
        slice.copy_from_slice(&DIRT);

        mem.snap().expect("Snapshotting should succeed");

        assert_eq!(mem.dirty_pages().count(), 0);
        let slice = &mem[OFFSET..][..DIRT.len()];
        assert_eq!(slice, DIRT, "Slice should be dirt just written");

        let slice = &mut mem[OFFSET..][..DIRT2.len()];
        slice.copy_from_slice(&DIRT2);

        mem.snap().expect("Snapshotting should succeed");

        assert_eq!(mem.dirty_pages().count(), 0);
        let slice = &mem[OFFSET..][..DIRT2.len()];
        assert_eq!(slice, DIRT2, "Slice should be dirt just written");

        mem.revert().expect("Reverting should succeed");

        assert_eq!(mem.dirty_pages().count(), 3);
        let slice = &mem[OFFSET..][..DIRT2.len()];
        assert_eq!(slice, DIRT2, "Slice should be dirt written second");

        mem.revert().expect("Reverting should succeed");

        assert_eq!(mem.dirty_pages().count(), 3);
        let slice = &mem[OFFSET..][..DIRT.len()];
        assert_eq!(slice, DIRT, "Slice should be dirt written first");
    }

    #[test]
    fn apply() {
        let mut mem = Mmap::new(N_PAGES, PAGE_SIZE)
            .expect("Instantiating new memory should succeed");

        let slice = &mut mem[OFFSET..][..DIRT.len()];
        slice.copy_from_slice(&DIRT);

        mem.snap().expect("Snapshotting should succeed");

        assert_eq!(mem.dirty_pages().count(), 0);
        let slice = &mem[OFFSET..][..DIRT.len()];
        assert_eq!(slice, DIRT, "Slice should be dirt just written");

        // Writing to the same page should now be reversible
        let slice = &mut mem[OFFSET..][..DIRT.len()];
        slice.copy_from_slice(&[0; 2 * PAGE_SIZE]);

        mem.apply().expect("Applying should succeed");

        assert_eq!(mem.dirty_pages().count(), 3);
        let slice = &mut mem[OFFSET..][..DIRT.len()];
        assert_eq!(
            slice,
            &[0; 2 * PAGE_SIZE],
            "Slice should be the zeros written afterwards"
        );
    }

    #[test]
    fn apply_revert_apply() {
        const N_WRITES: usize = 64;
        let mut rng = StdRng::seed_from_u64(0xDEAD_BEEF);

        let mut mem = Mmap::new(N_PAGES, PAGE_SIZE)
            .expect("Instantiating new memory should succeed");
        let mut mem_alt = Mmap::new(N_PAGES, PAGE_SIZE)
            .expect("Instantiating new memory should succeed");

        // Snapshot both, make the same changes on both, and apply the changes.
        mem.snap().expect("Snapshotting should succeed");
        mem_alt.snap().expect("Snapshotting should succeed");

        for _ in 0..N_WRITES {
            let i = rng.gen_range(0..N_PAGES);
            let byte = rng.gen();

            mem[i] = byte;
            mem_alt[i] = byte;
        }

        mem.apply().expect("Applying should succeed");
        mem_alt.apply().expect("Applying should succeed");

        // Snapshot one, make some changes, and revert it.
        mem.snap().expect("Snapshotting should succeed");
        for _ in 0..N_WRITES {
            let i = rng.gen_range(0..N_PAGES);
            let byte = rng.gen();
            mem[i] = byte;
        }
        mem.revert().expect("Reverting should succeed");

        // Snapshot both, make the same changes on both, and apply the changes.
        mem.snap().expect("Snapshotting should succeed");
        mem_alt.snap().expect("Snapshotting should succeed");

        for _ in 0..N_WRITES {
            let i = rng.gen_range(0..N_PAGES);
            let byte = rng.gen();

            mem[i] = byte;
            mem_alt[i] = byte;
        }

        mem.apply().expect("Applying should succeed");
        mem_alt.apply().expect("Applying should succeed");

        mem.dirty_pages().zip(mem_alt.dirty_pages()).for_each(
            |((dirty, clean, index), (alt_dirty, alt_clean, alt_index))| {
                let hash_dirty = blake3::hash(dirty);
                let hash_alt_dirty = blake3::hash(alt_dirty);

                let hash_dirty = hex::encode(hash_dirty.as_bytes());
                let hash_alt_dirty = hex::encode(hash_alt_dirty.as_bytes());

                assert_eq!(
                    hash_dirty, hash_alt_dirty,
                    "Dirty state should be the same"
                );

                let hash_clean = blake3::hash(clean);
                let hash_alt_clean = blake3::hash(alt_clean);

                let hash_clean = hex::encode(hash_clean.as_bytes());
                let hash_alt_clean = hex::encode(hash_alt_clean.as_bytes());

                assert_eq!(
                    hash_clean, hash_alt_clean,
                    "Clean state should be the same"
                );

                assert_eq!(index, alt_index, "Index should be the same");
            },
        );
    }
}
