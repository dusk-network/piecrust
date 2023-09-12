// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::fs::OpenOptions;
use std::io;
use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::ptr::NonNull;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use crumbles::{Mmap, PAGE_SIZE};
use wasmer::WASM_MAX_PAGES;
use wasmer_types::{MemoryType, Pages};
use wasmer_vm::{
    initialize_memory_with_data, LinearMemory, MemoryError, MemoryStyle, Trap,
    VMMemory, VMMemoryDefinition,
};

const MIN_PAGES: usize = 4;
const MIN_MEM_SIZE: usize = MIN_PAGES * PAGE_SIZE;
const MAX_PAGES: usize = WASM_MAX_PAGES as usize;
pub const MAX_MEM_SIZE: usize = MAX_PAGES * PAGE_SIZE;

#[derive(Debug)]
pub(crate) struct MemoryInner {
    pub(crate) mmap: Mmap,
    pub(crate) def: VMMemoryDefinition,
    init: bool,
}

/// WASM memory belonging to a given contract during a given session.
#[derive(Debug, Clone)]
pub struct Memory {
    inner: Arc<RwLock<MemoryInner>>,
}

impl Memory {
    pub(crate) fn new() -> io::Result<Self> {
        let mut mmap = Mmap::new()?;

        let def = VMMemoryDefinition {
            base: mmap.as_mut_ptr(),
            current_length: MIN_MEM_SIZE,
        };

        Ok(Self {
            inner: Arc::new(RwLock::new(MemoryInner {
                mmap,
                def,
                init: false,
            })),
        })
    }

    pub(crate) fn from_files<P, I>(paths: I, len: usize) -> io::Result<Self>
    where
        P: AsRef<Path>,
        I: IntoIterator<Item = (usize, P)>,
    {
        let mut mmap = unsafe {
            Mmap::with_files(paths.into_iter().map(|(offset, path)| {
                OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(path)
                    .map(|file| (offset, file))
            }))?
        };

        let def = VMMemoryDefinition {
            base: mmap.as_mut_ptr(),
            current_length: len,
        };

        Ok(Self {
            inner: Arc::new(RwLock::new(MemoryInner {
                mmap,
                def,
                init: true,
            })),
        })
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
    pub(crate) inner: RwLockReadGuard<'a, MemoryInner>,
}

impl<'a> AsRef<[u8]> for MemoryReadGuard<'a> {
    fn as_ref(&self) -> &[u8] {
        &self.inner.mmap
    }
}

impl<'a> Deref for MemoryReadGuard<'a> {
    type Target = Mmap;

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
    type Target = Mmap;

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
        MemoryType {
            minimum: Pages(MIN_PAGES as u32),
            maximum: Some(Pages(MAX_PAGES as u32)),
            shared: false,
        }
    }

    fn size(&self) -> Pages {
        let pages = self.read().inner.def.current_length / PAGE_SIZE;
        Pages(pages as u32)
    }

    fn style(&self) -> MemoryStyle {
        MemoryStyle::Static {
            bound: Pages(MAX_PAGES as u32),
            offset_guard_size: 0,
        }
    }

    fn grow(&mut self, delta: Pages) -> Result<Pages, MemoryError> {
        let mut memory = self.write();

        let current_len = memory.inner.def.current_length;
        let new_len = current_len + delta.0 as usize * PAGE_SIZE;

        if new_len > MAX_PAGES * PAGE_SIZE {
            return Err(MemoryError::CouldNotGrow {
                current: Pages((current_len / PAGE_SIZE) as u32),
                attempted_delta: delta,
            });
        }

        memory.inner.def = VMMemoryDefinition {
            base: memory.as_mut_ptr(),
            current_length: new_len,
        };

        Ok(Pages((new_len / PAGE_SIZE) as u32))
    }

    fn vmmemory(&self) -> NonNull<VMMemoryDefinition> {
        let inner = self.inner.read().unwrap();
        let ptr = &inner.def as *const VMMemoryDefinition;
        NonNull::new(ptr as *mut VMMemoryDefinition).unwrap()
    }

    fn try_clone(&self) -> Option<Box<dyn LinearMemory + 'static>> {
        Some(Box::new(self.clone()))
    }

    unsafe fn initialize_with_data(
        &self,
        start: usize,
        data: &[u8],
    ) -> Result<(), Trap> {
        let this = self.write();
        let mut inner = this.inner;

        match inner.init {
            true => Ok(()),
            false => {
                initialize_memory_with_data(&inner.def, start, data).map(|_| {
                    inner.init = true;
                })
            }
        }
    }
}

impl From<Memory> for VMMemory {
    fn from(memory: Memory) -> Self {
        VMMemory(Box::new(memory))
    }
}
