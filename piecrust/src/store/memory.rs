// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::sync::atomic::Ordering;
use std::{
    fmt::{Debug, Formatter},
    io,
    ops::{Deref, DerefMut},
    sync::atomic::AtomicUsize,
};

use crumbles::{LocateFile, Mmap};
use dusk_wasmtime::LinearMemory;

pub const PAGE_SIZE: usize = 0x10000;

const WASM32_MAX_PAGES: usize = 0x10000;
const WASM64_MAX_PAGES: usize = 0x4000000;

pub struct MemoryInner {
    pub mmap: Mmap,
    pub current_len: usize,
    pub is_new: bool,
    is_64: bool,
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
    pub fn new(is_64: bool) -> io::Result<Self> {
        let max_pages = if is_64 {
            WASM64_MAX_PAGES
        } else {
            WASM32_MAX_PAGES
        };

        Ok(Self {
            inner: Box::leak(Box::new(MemoryInner {
                mmap: Mmap::new(max_pages, PAGE_SIZE)?,
                current_len: 0,
                is_new: true,
                is_64,
                ref_count: AtomicUsize::new(1),
            })),
        })
    }

    pub fn from_files<FL>(
        is_64: bool,
        file_locator: FL,
        len: usize,
    ) -> io::Result<Self>
    where
        FL: 'static + LocateFile,
    {
        let max_pages = if is_64 {
            WASM64_MAX_PAGES
        } else {
            WASM32_MAX_PAGES
        };

        Ok(Self {
            inner: Box::leak(Box::new(MemoryInner {
                mmap: unsafe {
                    Mmap::with_files(max_pages, PAGE_SIZE, file_locator)?
                },
                current_len: len,
                is_new: false,
                is_64,
                ref_count: AtomicUsize::new(1),
            })),
        })
    }

    pub fn is_64(&self) -> bool {
        self.inner.is_64
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

    fn grow_to(&mut self, new_size: usize) -> Result<(), dusk_wasmtime::Error> {
        self.inner.current_len = new_size;
        Ok(())
    }

    fn as_ptr(&self) -> *mut u8 {
        self.inner.as_ptr() as _
    }

    fn byte_capacity(&self) -> usize {
        self.inner.len()
    }

    fn needs_init(&self) -> bool {
        self.is_new
    }
}
