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
    fs::OpenOptions,
    mem::{self, MaybeUninit},
    ops::{Deref, DerefMut},
    os::fd::AsRawFd,
    path::PathBuf,
    sync::{Once, OnceLock, RwLock},
    {io, process, ptr, slice},
};

use libc::{
    c_int, sigaction, sigemptyset, siginfo_t, sigset_t, ucontext_t,
    MAP_ANONYMOUS, MAP_FAILED, MAP_FIXED, MAP_NORESERVE, MAP_PRIVATE,
    PROT_NONE, PROT_READ, PROT_WRITE, SA_SIGINFO,
};

/// A handle to a copy-on-write memory-mapped region that keeps track of which
/// pages have been written to.
///
/// A `Mmap` may be backed by a set of files, physical memory, or a combination
/// of both. Use [`new`] to create a new mmap backed entirely by physical
/// memory, and [`with_files`] to create a new mmap backed by the given
/// set of files, at the given offsets.
///
/// It is possible to create snapshots of the memory, which can be used to
/// revert to a previous state. See [`snap`] for more details.
///
/// Writes are tracked at the page level. This functions as follows:
///
/// - When a region is first mapped, its permissions are set to read-only,
///   resulting in a segmentation fault when a write is attempted.
/// - When a write is attempted the segmentation fault is caught using a signal
///   handler, which proceeds to set the permissions of the page to read-write
///   while also marking it as dirty in the mmap.
///
/// `Mmap` is [`Sync`] and [`Send`].
///
/// [`new`]: Mmap::new
/// [`with_files`]: Mmap::with_files
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
        unsafe { Self::with_files(page_number, page_size, |_| None) }
    }

    /// Create a new mmap, backed partially by physical memory, and partially
    /// the files opened by the given file locator. The `file_locator` is a
    /// closure taking a page index and optionally returning the file meant to
    /// be used as the backing for that page.
    ///
    /// The size of the memory region is specified by the caller using the
    /// number of pages - `page_number` - and the page size - `page_size`. The
    /// size total size of the region will then be `page_number * page_size`.
    ///
    /// # Errors
    /// If the underlying call to map memory fails, the function will return an
    /// error. The files given by the `file_locator` must be guaranteed to
    /// exist, otherwise a segmentation fault will occur.
    ///
    /// # Safety
    /// The caller must ensure that the given files are not modified while
    /// they're mapped. Modifying a file while it's mapped will result in
    /// *Undefined Behavior* (UB).
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
    /// let mut file = File::open("LICENSE")?;
    ///
    /// let mut contents = Vec::new();
    /// file.read_to_end(&mut contents)?;
    ///
    /// let mmap = unsafe {
    ///     Mmap::with_files(65536, 65536, move |page_index| {
    ///         match page_index {
    ///             0 => Some(PathBuf::from("LICENSE")),
    ///             _ => None,
    ///         }
    ///     })?
    /// };
    /// assert_eq!(mmap[..contents.len()], contents[..]);
    /// # Ok(())
    /// # }
    /// ```
    pub unsafe fn with_files<FL>(
        page_number: usize,
        page_size: usize,
        file_locator: FL,
    ) -> io::Result<Self>
    where
        FL: 'static + LocateFile,
    {
        let inner = MmapInner::new(page_number, page_size, file_locator)?;

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
    static PAGE_SIZE: OnceLock<usize> = OnceLock::new();
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

/// Types that can used to locate a file for a given page.
pub trait LocateFile: Send + Sync {
    /// Locate a file for the given page index.
    ///
    /// # Errors
    /// The function may return an error to signal that there was a problem
    /// looking up a file for the given page index, but should use `Ok(None)`
    /// when there is no file for the given page index.
    fn locate_file(&mut self, page_index: usize) -> Option<PathBuf>;
}

impl<F> LocateFile for F
where
    F: FnMut(usize) -> Option<PathBuf>,
    F: Send + Sync,
{
    fn locate_file(&mut self, page_index: usize) -> Option<PathBuf> {
        self(page_index)
    }
}

/// Contains the actual memory region, together with the set of dirty pages.
struct MmapInner {
    bytes: &'static mut [u8],

    page_size: usize,
    page_number: usize,

    mapped_pages: PageBits,
    snapshots: Vec<Snapshot>,

    file_locator: Box<dyn LocateFile>,
}

impl MmapInner {
    unsafe fn new<FL>(
        page_number: usize,
        page_size: usize,
        file_locator: FL,
    ) -> io::Result<Self>
    where
        FL: 'static + LocateFile,
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

        Ok(Self {
            bytes,
            page_size,
            page_number,
            mapped_pages,
            // There should always be at least one snapshot
            snapshots: vec![snapshot],
            file_locator: Box::new(file_locator),
        })
    }

    /// Processes a segfault at the given address. The address must be
    /// guaranteed to be within the memory region.
    ///
    /// Before segfaulting, the entire memory region is protected with
    /// `PROT_NONE`. When either a read or a write is attempted, a page will be
    /// mapped onto the accessed memory, and the permissions will then be set to
    /// `PROT_READ` for that page. If a write is attempted, a new segfault will
    /// occur, and the permissions will be set to `PROT_READ | PROT_WRITE` for
    /// that page.
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

        // Map the file given by the file locator, if any, at the given offset.
        // If we've already mapped it, we don't need to do so again.
        self.mapped_pages.set_and_exec(
            page_index,
            |is_bit_set| -> io::Result<()> {
                if is_bit_set {
                    return Ok(());
                }

                if let Some(path) = self.file_locator.locate_file(page_index) {
                    let file =
                        OpenOptions::new().read(true).write(true).open(path)?;

                    let ptr = libc::mmap(
                        page_addr as _,
                        page_size,
                        PROT_NONE,
                        MAP_PRIVATE | MAP_FIXED | MAP_NORESERVE,
                        file.as_raw_fd(),
                        0,
                    );

                    if ptr == MAP_FAILED {
                        return Err(io::Error::last_os_error());
                    }
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
