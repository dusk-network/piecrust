// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Library for creating and managing [`MEM_SIZE`] bytes copy-on-write
//! memory-mapped regions.
//!
//! The core functionality is offered by the [`Mmap`] struct, which is a
//! read-write memory region that keeps track of which pages have been written
//! to.
//!
//! Each `Mmap` is `MEM_SIZE` in size, and can be backed by physical memory, a
//! set of files, or a combination of both. Each page is [`PAGE_SIZE`] in size,
//! meaning that each mmap contains 65536 pages.
//!
//! # Example
//! ```rust
//! use crumbles::Mmap;
//!
//! let mut mmap = Mmap::new().unwrap();
//!
//! // When first created, the mmap is not dirty.
//! assert_eq!(mmap.dirty_pages().count(), 0);
//!
//! mmap[24] = 42;
//! // After writing a single byte, the page it's on is dirty.
//! assert_eq!(mmap.dirty_pages().count(), 1);
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
    collections::BTreeMap,
    fs::File,
    mem::{self, MaybeUninit},
    ops::{Deref, DerefMut},
    os::fd::IntoRawFd,
    sync::{Once, OnceLock, RwLock},
    {io, process, ptr, slice},
};

use libc::{
    c_int, sigaction, sigemptyset, siginfo_t, sigset_t, ucontext_t,
    MAP_ANONYMOUS, MAP_FAILED, MAP_FIXED, MAP_NORESERVE, MAP_PRIVATE,
    PROT_READ, PROT_WRITE, SA_SIGINFO,
};

/// Size of each memory region in bytes. 4GiB.
pub const MEM_SIZE: usize = 0x100_000_000;
/// Size of each memory page in bytes. 64KiB.
pub const PAGE_SIZE: usize = 0x10_000;

/// A handle to a [`MEM_SIZE`] copy-on-write memory-mapped region that keeps
/// track of which pages have been written to. Each page is [`PAGE_SIZE`] in
/// size.
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
#[derive(Debug)]
pub struct Mmap(&'static mut MmapInner);

impl Mmap {
    /// Create a new mmap, backed entirely by physical memory. The memory is
    /// initialized to all zeros.
    ///
    /// # Errors
    /// If the underlying call to map memory fails, the function will return an
    /// error.
    ///
    /// # Example
    /// ```rust
    /// use crumbles::Mmap;
    ///
    /// let mmap = Mmap::new().unwrap();
    /// assert_eq!(mmap[..0x10_000], [0; 0x10_000]);
    /// ```
    pub fn new() -> io::Result<Self> {
        unsafe { Self::with_files(None) }
    }

    /// Create a new memory, backed by the given files at the given offsets.
    /// Regions of the memory not covered
    ///
    /// # Errors
    /// If the given files are too large for the [`MEM_SIZE`] memory region, or
    /// at an offset where they wouldn't fit within said region, the function
    /// will return an error. Also, if any of the files' size or offsets is not
    /// a multiple of the page size, the function will return an error.
    ///
    /// # Safety
    /// The caller must ensure that the given files are not modified while
    /// they're mapped. Modifying a file while it's mapped will result in
    /// *Undefined Behavior* (UB).
    ///
    /// # Example
    /// ```rust
    /// use std::fs::File;
    /// use std::io::Read;
    /// use std::iter;
    ///
    /// use crumbles::Mmap;
    ///
    /// let mut file = File::open("LICENSE").unwrap();
    ///
    /// let mut contents = Vec::new();
    /// file.read_to_end(&mut contents).unwrap();
    ///
    /// let mmap = unsafe { Mmap::with_files(iter::once(Ok((file, 0)))) }.unwrap();
    /// assert_eq!(mmap[..contents.len()], contents[..]);
    /// ```
    pub unsafe fn with_files<I>(files_and_offsets: I) -> io::Result<Self>
    where
        I: IntoIterator<Item = io::Result<(File, usize)>>,
    {
        let inner = MmapInner::with_files(files_and_offsets)?;

        with_global_map_mut(|global_map| {
            let inner = Box::leak(Box::new(inner));

            let start_addr = inner.bytes.as_mut_ptr() as usize;
            let end_addr = start_addr + MEM_SIZE;

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
    /// If the given files are too large for the [`MEM_SIZE`] memory region, or
    /// at an offset where they wouldn't fit within said region, the function
    /// will return an error. Also, if any of the files' size or offsets is not
    /// a multiple of the page size, the function will return an error.
    ///
    /// # Example
    /// ```rust
    /// use crumbles::Mmap;
    ///
    /// let mut mmap = Mmap::new().unwrap();
    ///
    /// mmap[0] = 1;
    /// mmap.snap().unwrap();
    ///
    /// // Snapshotting the memory keeps the current state, and also resets
    /// // dirty pages to clean.
    /// assert_eq!(mmap[0], 1);
    /// assert_eq!(mmap.dirty_pages().count(), 0);
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
    /// If the given files are too large for the [`MEM_SIZE`] memory region, or
    /// at an offset where they wouldn't fit within said region, the function
    /// will return an error. Also, if any of the files' size or offsets is not
    /// a multiple of the page size, the function will return an error.
    ///
    /// # Example
    /// ```rust
    /// use crumbles::Mmap;
    ///
    /// let mut mmap = Mmap::new().unwrap();
    ///
    /// mmap[0] = 1;
    /// mmap.revert().unwrap();
    ///
    /// assert_eq!(mmap[0], 0);
    /// assert_eq!(mmap.dirty_pages().count(), 0);
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
    /// use crumbles::Mmap;
    ///
    /// let mut mmap = Mmap::new().unwrap();
    ///
    /// mmap[0] = 1;
    /// mmap.apply().unwrap();
    ///
    /// assert_eq!(mmap[0], 1);
    /// assert_eq!(mmap.dirty_pages().count(), 1);
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
    /// use crumbles::Mmap;
    ///
    /// let mut mmap = Mmap::new().unwrap();
    /// mmap[0x10_000] = 1; // second page
    ///
    /// let dirty_pages: Vec<_> = mmap.dirty_pages().collect();
    ///
    /// assert_eq!(dirty_pages.len(), 1);
    /// assert_eq!(dirty_pages[0].2, 0x10_000, "Offset to the first page");
    /// ```
    pub fn dirty_pages(&self) -> impl Iterator<Item = (&[u8], &[u8], usize)> {
        self.0
            .last_snapshot()
            .iter()
            .map(move |(page_num, clean_page)| {
                let offset = page_num * PAGE_SIZE;
                (
                    &self.0.bytes[offset..][..PAGE_SIZE],
                    &clean_page[..],
                    offset,
                )
            })
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
                let end_addr = start_addr + MEM_SIZE;

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

/// A map from dirty page numbers to their "clean" contents.
type Snapshot = BTreeMap<usize, Vec<u8>>;

/// Contains the actual memory region, together with the set of dirty pages.
#[derive(Debug)]
struct MmapInner {
    bytes: &'static mut [u8],
    snapshots: Vec<Snapshot>,
}

impl MmapInner {
    unsafe fn new() -> io::Result<Self> {
        setup_action();

        let bytes = {
            let ptr = libc::mmap(
                ptr::null_mut(),
                MEM_SIZE,
                PROT_READ,
                MAP_PRIVATE | MAP_ANONYMOUS | MAP_NORESERVE,
                -1,
                0,
            );

            if ptr == MAP_FAILED {
                return Err(io::Error::last_os_error());
            }

            slice::from_raw_parts_mut(ptr.cast(), MEM_SIZE)
        };

        Ok(Self {
            bytes,
            // There should always be at least one snapshot
            snapshots: vec![Snapshot::new()],
        })
    }

    unsafe fn with_files<I>(files_and_offsets: I) -> io::Result<Self>
    where
        I: IntoIterator<Item = io::Result<(File, usize)>>,
    {
        let inner = MmapInner::new()?;

        for r in files_and_offsets {
            let (file, offset) = r?;

            // Since we only build for 64-bit targets, we can safely assume
            // *neither* truncation *nor* wrapping will happen.
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_possible_wrap
            )]
            {
                let file_len = file.metadata()?.len() as usize;

                if offset + file_len > MEM_SIZE {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "File too large for memory region",
                    ));
                }

                let addr = inner.bytes.as_mut_ptr().add(offset);

                let ptr = libc::mmap(
                    addr.cast(),
                    file_len,
                    PROT_READ,
                    MAP_PRIVATE | MAP_FIXED | MAP_NORESERVE,
                    file.into_raw_fd(),
                    0,
                );

                if ptr == MAP_FAILED {
                    return Err(io::Error::last_os_error());
                }
            }
        }

        Ok(inner)
    }

    unsafe fn set_dirty(&mut self, page_num: usize) -> io::Result<()> {
        let start_addr = self.bytes.as_ptr() as usize;

        let page_offset = page_num * PAGE_SIZE;
        let page_addr = start_addr + page_offset;

        if libc::mprotect(page_addr as _, PAGE_SIZE, PROT_READ | PROT_WRITE)
            != 0
        {
            return Err(io::Error::last_os_error());
        }

        if !self.last_snapshot().contains_key(&page_num) {
            let mut clean_page = vec![0; PAGE_SIZE];
            clean_page.copy_from_slice(&self.bytes[page_offset..][..PAGE_SIZE]);

            self.last_snapshot_mut().insert(page_num, clean_page);
        }

        Ok(())
    }

    unsafe fn snap(&mut self) -> io::Result<()> {
        if libc::mprotect(self.bytes.as_mut_ptr().cast(), PAGE_SIZE, PROT_READ)
            != 0
        {
            return Err(io::Error::last_os_error());
        }

        self.snapshots.push(Snapshot::new());

        Ok(())
    }

    unsafe fn apply(&mut self) -> io::Result<()> {
        if libc::mprotect(self.bytes.as_mut_ptr().cast(), PAGE_SIZE, PROT_READ)
            != 0
        {
            return Err(io::Error::last_os_error());
        }

        let popped_snapshot = self
            .snapshots
            .pop()
            .expect("There should always be at least one snapshot");
        if self.snapshots.is_empty() {
            self.snapshots.push(Snapshot::new());
        }
        let snapshot = self.last_snapshot_mut();

        for (page_num, clean_page) in popped_snapshot {
            snapshot.entry(page_num).or_insert(clean_page);
        }

        Ok(())
    }

    unsafe fn revert(&mut self) -> io::Result<()> {
        let popped_snapshot = self
            .snapshots
            .pop()
            .expect("There should always be at least one snapshot");
        if self.snapshots.is_empty() {
            self.snapshots.push(Snapshot::new());
        }

        for (page_num, clean_page) in popped_snapshot {
            let page_offset = page_num * PAGE_SIZE;
            self.bytes[page_offset..][..PAGE_SIZE]
                .copy_from_slice(&clean_page[..]);
        }

        if libc::mprotect(self.bytes.as_mut_ptr().cast(), PAGE_SIZE, PROT_READ)
            != 0
        {
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
            libc::munmap(self.bytes.as_mut_ptr().cast(), MEM_SIZE);
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

            let start_addr = inner.bytes.as_mut_ptr() as usize;
            let page_num = (si_addr - start_addr) / PAGE_SIZE;

            if inner.set_dirty(page_num).is_err() {
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

    use std::thread;

    const DIRT: [u8; 2 * PAGE_SIZE] = [42; 2 * PAGE_SIZE];
    const OFFSET: usize = PAGE_SIZE / 2 + PAGE_SIZE;

    #[test]
    fn write() {
        let mut mem =
            Mmap::new().expect("Instantiating new memory should succeed");

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
                let mut mem = Mmap::new()
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
}
