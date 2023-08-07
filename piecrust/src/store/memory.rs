// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::ffi::{c_int, c_void};
use std::fs::File;
use std::ops::{Deref, DerefMut};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::ptr::NonNull;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::{cmp, fs, io, ptr, slice};

use flate2::read::DeflateDecoder;
use wasmer::{WASM_MAX_PAGES, WASM_PAGE_SIZE};
use wasmer_types::{MemoryType, Pages};
use wasmer_vm::{
    initialize_memory_with_data, LinearMemory, MemoryError, MemoryStyle, Trap,
    VMMemory, VMMemoryDefinition,
};

use libc::{
    off_t, MAP_ANONYMOUS, MAP_FAILED, MAP_FIXED, MAP_NORESERVE, MAP_PRIVATE,
    PROT_READ, PROT_WRITE,
};

use crate::store::diff::patch;

const WASM_MIN_PAGES: usize = 4;
pub const MIN_MEM_SIZE: usize = WASM_PAGE_SIZE * WASM_MIN_PAGES;
const MAX_MEM_SIZE: usize = WASM_PAGE_SIZE * WASM_MAX_PAGES as usize;

#[derive(Debug)]
struct MemoryInner {
    mmap: MemoryMmap,
    init: bool,
}

/// WASM memory belonging to a given contract during a given session.
#[derive(Debug, Clone)]
pub struct Memory {
    inner: Arc<RwLock<MemoryInner>>,
}

impl Memory {
    pub(crate) fn new() -> io::Result<Self> {
        let mmap = MemoryMmap::new()?;
        Ok(Self {
            inner: Arc::new(RwLock::new(MemoryInner { mmap, init: false })),
        })
    }

    pub(crate) fn from_file<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let mmap = MemoryMmap::map(path)?;
        Ok(Self {
            inner: Arc::new(RwLock::new(MemoryInner { mmap, init: true })),
        })
    }

    pub(crate) fn from_file_and_diff<P: AsRef<Path>>(
        path: P,
        diff_path: P,
    ) -> io::Result<Self> {
        let mmap_old = Self::from_file(&path)?;
        let mmap = Self::from_file(&path)?;

        let diff_file = File::open(diff_path)?;
        let mut decoder = DeflateDecoder::new(diff_file);

        patch(mmap_old, &mmap, &mut decoder)?;

        Ok(mmap)
    }

    pub fn read(&self) -> MemoryReadGuard {
        let inner = self.inner.read().unwrap();
        MemoryReadGuard { inner }
    }

    pub fn write(&self) -> MemoryWriteGuard {
        let inner = self.inner.write().unwrap();
        MemoryWriteGuard { inner }
    }
}

pub struct MemoryReadGuard<'a> {
    inner: RwLockReadGuard<'a, MemoryInner>,
}

impl<'a> AsRef<[u8]> for MemoryReadGuard<'a> {
    fn as_ref(&self) -> &[u8] {
        &self.inner.mmap
    }
}

impl<'a> Deref for MemoryReadGuard<'a> {
    type Target = MemoryMmap;

    fn deref(&self) -> &Self::Target {
        &self.inner.mmap
    }
}

pub struct MemoryWriteGuard<'a> {
    inner: RwLockWriteGuard<'a, MemoryInner>,
}

impl<'a> AsRef<[u8]> for MemoryWriteGuard<'a> {
    fn as_ref(&self) -> &[u8] {
        &self.inner.mmap
    }
}

impl<'a> AsMut<[u8]> for MemoryWriteGuard<'a> {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.inner.mmap
    }
}

impl<'a> Deref for MemoryWriteGuard<'a> {
    type Target = MemoryMmap;

    fn deref(&self) -> &Self::Target {
        &self.inner.mmap
    }
}

impl<'a> DerefMut for MemoryWriteGuard<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner.mmap
    }
}

impl LinearMemory for Memory {
    fn ty(&self) -> MemoryType {
        self.read().ty()
    }

    fn size(&self) -> Pages {
        self.read().size()
    }

    fn style(&self) -> MemoryStyle {
        self.read().style()
    }

    fn grow(&mut self, delta: Pages) -> Result<Pages, MemoryError> {
        self.write().grow(delta)
    }

    fn vmmemory(&self) -> NonNull<VMMemoryDefinition> {
        self.read().vmmemory()
    }

    fn try_clone(&self) -> Option<Box<dyn LinearMemory + 'static>> {
        self.read().try_clone()
    }

    unsafe fn initialize_with_data(
        &self,
        start: usize,
        data: &[u8],
    ) -> Result<(), Trap> {
        let mut this = self.write();
        let memory = this.vmmemory();

        match this.inner.init {
            true => Ok(()),
            false => initialize_memory_with_data(memory.as_ref(), start, data)
                .map(|_| {
                    this.inner.init = true;
                }),
        }
    }
}

impl From<Memory> for VMMemory {
    fn from(memory: Memory) -> Self {
        VMMemory(Box::new(memory))
    }
}

/// The memory used by a contract.
///
/// It consists of one large mmap that doesn't reserve swap space - see
/// [MAP_NORESERVE]. The memory region mapped will always be the maximum
/// possible, since virtual address space is cheap, but the writable area is
/// controlled by `len`.
///
/// [MAP_NORESERVE]: https://www.man7.org/linux/man-pages/man2/mmap.2.html
#[derive(Debug)]
#[repr(C)]
pub struct MemoryMmap {
    ptr: *mut u8,
    len: usize,
}

unsafe impl Send for MemoryMmap {}
unsafe impl Sync for MemoryMmap {}

impl Drop for MemoryMmap {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.ptr as *mut c_void, MAX_MEM_SIZE);
        }
    }
}

fn mmap(
    addr: *mut u8,
    len: usize,
    prot: c_int,
    flags: c_int,
    fd: c_int,
    offset: off_t,
) -> io::Result<*mut u8> {
    unsafe {
        let ptr = libc::mmap(addr as *mut c_void, len, prot, flags, fd, offset);
        if ptr == MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        Ok(ptr as *mut u8)
    }
}

impl MemoryMmap {
    /// Creates a new memory, backed by the system's physical memory.
    ///
    /// Memories created like this always have `MIN_MEM_SIZE`, unless
    /// [`grow_by`] is called.
    ///
    /// [`grow_by`]: MemoryMap::grow_by
    fn new() -> io::Result<Self> {
        let len = MIN_MEM_SIZE;

        let ptr = mmap(
            ptr::null_mut(),
            MAX_MEM_SIZE,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANONYMOUS | MAP_NORESERVE,
            -1,
            0,
        )?;

        // An offset guard page is added to optimize reads and writes.
        Ok(Self { ptr, len })
    }

    /// Maps the given file, creating it if it doesn't exist.
    ///
    /// Memories mapped from a file are mapped in two steps. In the first step
    /// the file is mmapped copy-on-write, reserving a region as large as
    /// `MAX_MEM_SIZE`. In the second step an overlapping region is mapped to
    /// physical memory, at an offset of file length, using `MAP_FIXED`. This
    /// ensures the memory is free to grow beyond the boundaries of the file
    /// into physical memory, in one contiguous memory region.
    ///
    /// # Errors
    /// If the file already exists, and it has a length that is not a multiple
    /// of WASM_PAGE_SIZE, the function will error.
    fn map<P: AsRef<Path>>(file: P) -> io::Result<Self> {
        let file = fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(file)?;

        let file_len = file.metadata()?.len();
        if file_len % WASM_PAGE_SIZE as u64 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "File length is not a multiple of WASM page size",
            ));
        }

        // Set the length of the file to the minimum if too small.
        let min_len = MIN_MEM_SIZE as u64;
        if file_len < min_len {
            file.set_len(min_len)?;
        }

        let fd = file.as_raw_fd();
        let file_len = cmp::max(file_len, min_len) as usize;

        let ptr = mmap(
            ptr::null_mut(),
            MAX_MEM_SIZE,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_NORESERVE,
            fd,
            0,
        )?;

        let growth_ptr = unsafe { ptr.add(file_len) };

        mmap(
            growth_ptr,
            MAX_MEM_SIZE - file_len,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANONYMOUS | MAP_NORESERVE | MAP_FIXED,
            -1,
            0,
        )?;

        Ok(Self { ptr, len: file_len })
    }

    /// Grow the map by the specified number of bytes.
    ///
    /// Growing in this scheme effectively means incrementing the length.
    pub fn grow_by(&mut self, delta_bytes: usize) -> io::Result<()> {
        let new_len = self.len + delta_bytes;

        if new_len > MAX_MEM_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Tried growing past the maximum number of bytes: {MAX_MEM_SIZE}"),
            ));
        }

        self.len = new_len;

        Ok(())
    }

    /// Return the memory as a byte slice.
    pub fn as_bytes(&self) -> &[u8] {
        let ptr = self.ptr as *const u8;
        unsafe { slice::from_raw_parts(ptr, self.len) }
    }

    /// Return the mmap as a mutable byte slice.
    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

impl Deref for MemoryMmap {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_bytes()
    }
}

impl DerefMut for MemoryMmap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_bytes_mut()
    }
}

impl LinearMemory for MemoryMmap {
    fn ty(&self) -> MemoryType {
        MemoryType {
            minimum: Pages(WASM_MIN_PAGES as u32),
            maximum: Some(Pages(WASM_MAX_PAGES - 1)), // there is one guard page
            shared: false,
        }
    }

    fn size(&self) -> Pages {
        let pages = self.len / WASM_PAGE_SIZE - 1; // there is one guard page
        Pages(pages as u32)
    }

    fn style(&self) -> MemoryStyle {
        MemoryStyle::Static {
            bound: Pages(WASM_MAX_PAGES),
            offset_guard_size: 1,
        }
    }

    fn grow(&mut self, delta: Pages) -> Result<Pages, MemoryError> {
        let current = self.size();
        let delta_bytes = delta.0 as usize * WASM_PAGE_SIZE;

        self.grow_by(delta_bytes)
            .map_err(|_| MemoryError::CouldNotGrow {
                current,
                attempted_delta: delta,
            })?;

        Ok(current + delta)
    }

    fn vmmemory(&self) -> NonNull<VMMemoryDefinition> {
        let ptr = self as *const Self;
        NonNull::new(ptr as *mut VMMemoryDefinition).unwrap()
    }

    fn try_clone(&self) -> Option<Box<dyn LinearMemory + 'static>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical() {
        let mut memory =
            MemoryMmap::new().expect("Creating map should succeed");

        memory[42] = 137;
        assert_eq!(memory[42], 137);
    }

    #[test]
    fn file() {
        let tmp_file = tempfile::NamedTempFile::new()
            .expect("Creating tempfile should succeed");
        let mut memory = MemoryMmap::map(&tmp_file)
            .expect("Mapping the file should succeed");

        memory[42] = 137;
        assert_eq!(memory[42], 137);
    }

    #[test]
    fn grow_physical() {
        let mut memory =
            MemoryMmap::new().expect("Creating map should succeed");

        let len = memory.len();
        let ones = vec![1; len];
        memory.copy_from_slice(&ones);

        memory.grow_by(10).expect("Growing should succeed");

        let new_len = memory.len();
        let twos = vec![2; new_len - len];

        memory[len..].copy_from_slice(&twos);

        let mut total_mem = ones;
        total_mem.extend(twos);

        assert_eq!(&memory[..], &total_mem[..]);
    }

    #[test]
    fn grow() {
        let tmp_file = tempfile::NamedTempFile::new()
            .expect("Creating tempfile should succeed");

        let mut memory = MemoryMmap::map(&tmp_file)
            .expect("Mapping the file should succeed");

        let len = memory.len();
        let ones = vec![1; len];
        memory.copy_from_slice(&ones);

        memory.grow_by(10).expect("Growing should succeed");

        let new_len = memory.len();
        let twos = vec![2; new_len - len];

        memory[len..].copy_from_slice(&twos);

        let mut total_mem = ones;
        total_mem.extend(twos);

        assert_eq!(&memory[..], &total_mem[..]);
    }
}
