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
    ptr::NonNull,
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
    inner: NonNull<MemoryInner>,
}

// SAFETY: `Memory` is moved across threads but accesses happen within
// wasmtime's single-threaded store execution model.
unsafe impl Send for Memory {}

// SAFETY: Clones share a `NonNull<MemoryInner>`, but execution guarantees
// no concurrent mutable access across clones.
unsafe impl Sync for Memory {}

impl Memory {
    pub fn new(is_64: bool) -> io::Result<Self> {
        let max_pages = if is_64 {
            WASM64_MAX_PAGES
        } else {
            WASM32_MAX_PAGES
        };

        let inner = Box::leak(Box::new(MemoryInner {
            mmap: Mmap::new(max_pages, PAGE_SIZE)?,
            current_len: 0,
            is_new: true,
            is_64,
            ref_count: AtomicUsize::new(1),
        }));

        Ok(Self {
            inner: NonNull::from(inner),
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

        let inner = Box::leak(Box::new(MemoryInner {
            mmap: unsafe {
                Mmap::with_files(max_pages, PAGE_SIZE, file_locator)?
            },
            current_len: len,
            is_new: false,
            is_64,
            ref_count: AtomicUsize::new(1),
        }));

        Ok(Self {
            inner: NonNull::from(inner),
        })
    }

    fn inner(&self) -> &MemoryInner {
        unsafe { self.inner.as_ref() }
    }

    fn inner_mut(&mut self) -> &mut MemoryInner {
        unsafe { self.inner.as_mut() }
    }

    pub fn is_64(&self) -> bool {
        self.inner().is_64
    }

    pub fn is_new(&self) -> bool {
        self.inner().is_new
    }

    pub fn set_is_new(&mut self, is_new: bool) {
        self.inner_mut().is_new = is_new;
    }

    pub fn current_len(&self) -> usize {
        self.inner().current_len
    }

    pub fn set_current_len(&mut self, len: usize) {
        self.inner_mut().current_len = len;
    }

    pub fn with_bytes<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        f(&self.inner().mmap)
    }

    pub fn with_bytes_mut<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        f(&mut self.inner_mut().mmap)
    }

    pub fn snap(&mut self) -> io::Result<()> {
        self.inner_mut().mmap.snap()
    }

    pub fn revert(&mut self) -> io::Result<()> {
        self.inner_mut().mmap.revert()
    }

    pub fn apply(&mut self) -> io::Result<()> {
        self.inner_mut().mmap.apply()
    }
}

/// SAFETY: Cloning copies the shared `NonNull<MemoryInner>` and increments
/// the refcount. This relies on single-threaded store execution to prevent
/// concurrent mutable access through separate clones.
impl Clone for Memory {
    fn clone(&self) -> Self {
        self.inner().ref_count.fetch_add(1, Ordering::SeqCst);
        Self { inner: self.inner }
    }
}

impl Drop for Memory {
    fn drop(&mut self) {
        if self.inner().ref_count.fetch_sub(1, Ordering::SeqCst) == 1 {
            unsafe {
                let _ = Box::from_raw(self.inner.as_ptr());
            }
        }
    }
}

impl Deref for Memory {
    type Target = MemoryInner;

    fn deref(&self) -> &Self::Target {
        self.inner()
    }
}

unsafe impl LinearMemory for Memory {
    fn byte_size(&self) -> usize {
        self.current_len()
    }

    fn maximum_byte_size(&self) -> Option<usize> {
        Some(self.inner().len())
    }

    fn grow_to(&mut self, new_size: usize) -> Result<(), dusk_wasmtime::Error> {
        self.set_current_len(new_size);
        Ok(())
    }

    fn needs_init(&self) -> bool {
        self.is_new()
    }

    fn as_ptr(&self) -> *mut u8 {
        self.inner().as_ptr() as _
    }

    fn wasm_accessible(&self) -> Range<usize> {
        let begin = self.inner().mmap.as_ptr() as _;
        let len = self.current_len();
        let end = begin + len;

        begin..end
    }
}
