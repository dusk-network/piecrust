// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::sync::atomic::Ordering;
use std::{
    fmt::{Debug, Formatter},
    io,
    ops::{Deref, DerefMut, Range},
    sync::atomic::AtomicUsize,
};

use crumbles::{LocateFile, Mmap};
use dusk_wasmtime::LinearMemory;

pub const PAGE_SIZE: usize = 0x10000;
const WASM_MAX_PAGES: u32 = 0x4000000;

const MIN_PAGES: usize = 4;
const MIN_MEM_SIZE: usize = MIN_PAGES * PAGE_SIZE;
const MAX_PAGES: usize = WASM_MAX_PAGES as usize;

pub const MAX_MEM_SIZE: usize = MAX_PAGES * PAGE_SIZE;

pub struct MemoryInner {
    pub mmap: Mmap,
    pub current_len: usize,
    pub is_new: bool,
    ref_count: AtomicUsize,
}

impl Debug for MemoryInner {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryInner")
            .field("mmap", &self.mmap.as_ptr())
            .field("mmap_len", &self.mmap.len())
            .field("current_len", &self.current_len)
            .field("is_new", &self.is_new)
            .finish()
    }
}

impl Deref for MemoryInner {
    type Target = Mmap;

    fn deref(&self) -> &Self::Target {
        &self.mmap
    }
}

impl DerefMut for MemoryInner {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.mmap
    }
}

/// WASM memory belonging to a given contract during a given session.
#[derive(Debug)]
pub struct Memory {
    inner: &'static mut MemoryInner,
}

impl Memory {
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            inner: Box::leak(Box::new(MemoryInner {
                mmap: Mmap::new(MAX_PAGES, PAGE_SIZE)?,
                current_len: MIN_MEM_SIZE,
                is_new: true,
                ref_count: AtomicUsize::new(1),
            })),
        })
    }

    pub fn from_files<FL>(file_locator: FL, len: usize) -> io::Result<Self>
    where
        FL: 'static + LocateFile,
    {
        Ok(Self {
            inner: Box::leak(Box::new(MemoryInner {
                mmap: unsafe {
                    Mmap::with_files(MAX_PAGES, PAGE_SIZE, file_locator)?
                },
                current_len: len,
                is_new: false,
                ref_count: AtomicUsize::new(1),
            })),
        })
    }
}

/// This implementation of clone is dangerous, and must be accompanied by the
/// underneath implementation of `Drop`.
///
/// We do this to avoid locking the memory in any way when recursively offering
/// it to a session.
///
/// It is safe since we guarantee that there is no access contention - read or
/// write.
impl Clone for Memory {
    fn clone(&self) -> Self {
        self.ref_count.fetch_add(1, Ordering::SeqCst);

        let inner = self.inner as *const MemoryInner;
        let inner = inner as *mut MemoryInner;
        // SAFETY: we explicitly allow aliasing of the memory for internal
        // use.
        Self {
            inner: unsafe { &mut *inner },
        }
    }
}

impl Drop for Memory {
    fn drop(&mut self) {
        if self.ref_count.fetch_sub(1, Ordering::SeqCst) == 1 {
            unsafe {
                let _ = Box::from_raw(self.inner);
            }
        }
    }
}

impl Deref for Memory {
    type Target = MemoryInner;

    fn deref(&self) -> &Self::Target {
        self.inner
    }
}

impl DerefMut for Memory {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner
    }
}

unsafe impl LinearMemory for Memory {
    fn byte_size(&self) -> usize {
        self.inner.current_len
    }

    fn maximum_byte_size(&self) -> Option<usize> {
        Some(MAX_MEM_SIZE)
    }

    fn grow_to(&mut self, new_size: usize) -> Result<(), dusk_wasmtime::Error> {
        self.inner.current_len = new_size;
        Ok(())
    }

    fn needs_init(&self) -> bool {
        self.is_new
    }

    fn as_ptr(&self) -> *mut u8 {
        self.inner.as_ptr() as _
    }

    fn wasm_accessible(&self) -> Range<usize> {
        let begin = self.inner.mmap.as_ptr() as _;
        let len = self.inner.current_len;
        let end = begin + len;

        begin..end
    }
}
