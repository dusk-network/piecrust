// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::ops::{Deref, DerefMut};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::ptr;
use std::ptr::NonNull;
use std::{io, slice};

use libc::{
    MAP_ANONYMOUS, MAP_FAILED, MAP_FIXED, MAP_NORESERVE, MAP_PRIVATE,
    PROT_READ, PROT_WRITE,
};
use tempfile::tempfile;
use wasmer_types::{MemoryError, MemoryStyle, MemoryType, Pages};
use wasmer_vm::{LinearMemory, VMMemoryDefinition};

const WASM_PAGE_SIZE: usize = 65_536;
const WASM_MIN_PAGES: usize = 4;
const WASM_MAX_PAGES: usize = 65_536;

/// The size of the address space reserved for a memory.
const MMAP_SIZE: usize = WASM_MAX_PAGES * WASM_PAGE_SIZE;

unsafe impl Send for MmapInner {}
unsafe impl Sync for MmapInner {}

/// Mmap pointer and length matching the representation of a
/// [`VMMemoryDefinition`].
#[derive(Debug)]
#[repr(C)]
struct MmapInner {
    ptr: *mut libc::c_void,
    len: usize,
    mmap_len: usize,
}

impl MmapInner {
    /// Perform an `mmap` system call with the given parameters.
    fn map(
        addr: *mut libc::c_void,
        len: usize,
        mmap_len: usize,
        prot: libc::c_int,
        flags: libc::c_int,
        fd: libc::c_int,
    ) -> io::Result<Self> {
        Ok(Self {
            ptr: Self::_mmap(addr, mmap_len, prot, flags, fd)?,
            len,
            mmap_len,
        })
    }

    /// Remap the mmap - meaning use the same address.
    fn remap(
        &mut self,
        len: usize,
        mmap_len: usize,
        prot: libc::c_int,
        flags: libc::c_int,
        fd: libc::c_int,
    ) -> io::Result<()> {
        self.ptr = Self::_mmap(self.ptr, mmap_len, prot, flags, fd)?;
        self.len = len;

        Ok(())
    }

    /// Raw mmap call.
    fn _mmap(
        addr: *mut libc::c_void,
        len: usize,
        prot: libc::c_int,
        flags: libc::c_int,
        fd: libc::c_int,
    ) -> io::Result<*mut libc::c_void> {
        unsafe {
            let ptr = libc::mmap(addr, len, prot, flags, fd, 0);
            if ptr == MAP_FAILED {
                return Err(io::Error::last_os_error());
            }
            Ok(ptr)
        }
    }

    /// Return the mmap as a byte slice.
    fn as_bytes(&self) -> &[u8] {
        let ptr = self.ptr as *const u8;
        unsafe { slice::from_raw_parts(ptr, self.len) }
    }

    /// Return the mmap as a mutable byte slice.
    fn as_bytes_mut(&mut self) -> &mut [u8] {
        let ptr = self.ptr as *mut u8;
        unsafe { slice::from_raw_parts_mut(ptr, self.len) }
    }
}

/// When an Mmap is dropped, it is unmapped.
impl Drop for MmapInner {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.ptr, self.mmap_len);
        }
    }
}

/// A read-only mmap.
#[derive(Debug)]
pub struct Mmap(MmapInner);

impl Mmap {
    /// Create a new anonymous mmap filled with the given bytes.
    pub fn new<B: AsRef<[u8]>>(bytes: B) -> io::Result<Self> {
        let bytes = bytes.as_ref();

        let len = bytes.len();

        let mut inner = MmapInner::map(
            ptr::null_mut(),
            len,
            len,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANONYMOUS | MAP_NORESERVE,
            -1,
        )?;
        inner.as_bytes_mut().copy_from_slice(bytes);

        Ok(Self(inner))
    }

    /// Map a file at a given path read-only.
    pub fn map<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = File::open(path)?;
        let fd = file.as_raw_fd();

        let len = file.metadata()?.len() as usize;

        Ok(Self(MmapInner::map(
            ptr::null_mut(),
            len,
            len,
            PROT_READ,
            MAP_PRIVATE,
            fd,
        )?))
    }

    /// Return a slice to the underlying bytes.
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl Deref for Mmap {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_bytes()
    }
}

impl AsRef<[u8]> for Mmap {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

#[derive(Debug)]
pub struct MmapMut(MmapInner);

impl MmapMut {
    /// Creates a new mutable mmap, backed by a temporary file.
    pub fn new() -> io::Result<Self> {
        let file_len = WASM_MIN_PAGES * WASM_PAGE_SIZE;

        let file = tempfile()?;
        file.set_len(file_len as u64)?;
        let fd = file.as_raw_fd();

        Ok(Self(MmapInner::map(
            ptr::null_mut(),
            file_len,
            MMAP_SIZE,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_NORESERVE,
            fd,
        )?))
    }

    /// Creates a mutable mmap to the given file.
    pub fn map<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        let file_len = file.metadata()?.len() as usize;

        let fd = file.as_raw_fd();

        Ok(Self(MmapInner::map(
            ptr::null_mut(),
            file_len,
            MMAP_SIZE,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_NORESERVE,
            fd,
        )?))
    }

    /// Grow the mmap by `delta` bytes.
    pub fn grow_by(&mut self, delta: usize) -> io::Result<()> {
        let mut file = tempfile()?;
        let file_len = self.0.len + delta;

        file.set_len(file_len as u64)?;
        file.write_all(self)?;

        let fd = file.as_raw_fd();

        self.0.remap(
            file_len,
            MMAP_SIZE,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_FIXED | MAP_NORESERVE,
            fd,
        )?;

        Ok(())
    }

    /// Return a slice to the underlying bytes.
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    /// Return a mutable slice to the underlying bytes.
    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        self.0.as_bytes_mut()
    }
}

impl Deref for MmapMut {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_bytes()
    }
}

impl DerefMut for MmapMut {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_bytes_mut()
    }
}

impl AsRef<[u8]> for MmapMut {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl AsMut<[u8]> for MmapMut {
    fn as_mut(&mut self) -> &mut [u8] {
        self.as_bytes_mut()
    }
}

impl MmapMut {
    pub fn ty(&self) -> MemoryType {
        MemoryType {
            minimum: Pages(WASM_MIN_PAGES as u32),
            maximum: Some(Pages(WASM_MAX_PAGES as u32)),
            shared: false,
        }
    }

    pub fn size(&self) -> Pages {
        Pages((self.len() / WASM_PAGE_SIZE) as u32)
    }

    pub fn style(&self) -> MemoryStyle {
        MemoryStyle::Dynamic {
            offset_guard_size: 0,
        }
    }

    pub fn grow(&mut self, delta: Pages) -> Result<Pages, MemoryError> {
        let current_size = self.size();
        let pages = current_size + delta;

        self.grow_by(pages.0 as usize * WASM_PAGE_SIZE)
            .map_err(|_| MemoryError::CouldNotGrow {
                current: current_size,
                attempted_delta: delta,
            })?;

        Ok(pages)
    }

    pub fn vmmemory(&self) -> NonNull<VMMemoryDefinition> {
        let inner = &self.0 as *const MmapInner;
        NonNull::new(inner as *mut VMMemoryDefinition).unwrap()
    }

    pub fn try_clone(&self) -> Option<Box<dyn LinearMemory + 'static>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    const BYTES_PATH: &str = "../assets/mmap";
    const BYTES: &[u8] = include_bytes!("../../../assets/mmap");

    #[test]
    fn anonymous() -> io::Result<()> {
        let mmap = Mmap::new(BYTES)?;

        assert_eq!(&*mmap, BYTES, "Should contain placed bytes");

        Ok(())
    }

    #[test]
    fn map_read_only() -> io::Result<()> {
        let mmap = Mmap::map(BYTES_PATH)?;

        assert_eq!(&*mmap, BYTES, "File should contain original bytes");

        Ok(())
    }

    #[test]
    fn mutable() -> io::Result<()> {
        let mut mmap = MmapMut::new()?;

        let bytes = mmap.as_bytes_mut();
        bytes[..BYTES.len()].copy_from_slice(BYTES);

        assert_eq!(&bytes[..BYTES.len()], BYTES, "Should contain placed bytes");

        Ok(())
    }

    #[test]
    fn map_mutable() -> io::Result<()> {
        let mut mmap = MmapMut::map(BYTES_PATH)?;

        const NEW_BYTES: &[u8] = b"dead beef";

        let bytes = mmap.as_bytes_mut();
        bytes[..NEW_BYTES.len()].copy_from_slice(NEW_BYTES);

        let file_bytes = fs::read(BYTES_PATH)?;

        assert_eq!(
            &bytes[..NEW_BYTES.len()],
            NEW_BYTES,
            "Should contain placed bytes"
        );
        assert_eq!(
            &file_bytes[..BYTES.len()],
            BYTES,
            "File bytes should remain the same due to copy-on-write"
        );

        Ok(())
    }

    #[test]
    fn growth() -> io::Result<()> {
        let mut mmap = MmapMut::new()?;

        const DELTA: usize = 0xFF;

        let initial_bytes = mmap.as_bytes();
        let initial_ptr = initial_bytes.as_ptr();
        let initial_len = initial_bytes.len();

        mmap[..BYTES.len()].copy_from_slice(BYTES);
        mmap.grow_by(DELTA)?;

        let final_bytes = mmap.as_bytes();
        let final_ptr = final_bytes.as_ptr();
        let final_len = final_bytes.len();

        assert_eq!(
            initial_ptr, final_ptr,
            "Pointer should remain equal after growth"
        );
        assert_eq!(
            final_len,
            initial_len + DELTA,
            "Mmap should grow by specified amount"
        );
        assert_eq!(
            &final_bytes[..BYTES.len()],
            BYTES,
            "Contents should remain as set"
        );

        Ok(())
    }
}
