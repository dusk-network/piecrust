// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::fs::File;
use std::io;
use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::ptr::NonNull;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use flate2::read::DeflateDecoder;
use wasmer_types::{MemoryType, Pages};
use wasmer_vm::{
    initialize_memory_with_data, LinearMemory, MemoryError, MemoryStyle, Trap,
    VMMemory, VMMemoryDefinition,
};

use crate::store::diff::patch;
use crate::store::mmap::{Mmap, MmapMut};

/// Wasmer can only be considered to have initialized the memory when it has
/// called `LinearMemory::initialize_with_data` twice.
const INIT_COUNT: u8 = 2;

#[derive(Debug)]
struct MemoryInner {
    mmap: MmapMut,
    init_count: u8,
}

/// WASM memory belonging to a given contract during a given session.
#[derive(Debug, Clone)]
pub struct Memory {
    inner: Arc<RwLock<MemoryInner>>,
}

impl Memory {
    pub(crate) fn new() -> io::Result<Self> {
        let mmap = MmapMut::new()?;
        Ok(Self {
            inner: Arc::new(RwLock::new(MemoryInner {
                mmap,
                init_count: 0,
            })),
        })
    }

    pub(crate) fn from_file<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let mmap = MmapMut::map(path)?;
        Ok(Self {
            inner: Arc::new(RwLock::new(MemoryInner {
                mmap,
                init_count: INIT_COUNT,
            })),
        })
    }

    pub(crate) fn from_file_and_diff<P: AsRef<Path>>(
        path: P,
        diff_path: P,
    ) -> io::Result<Self> {
        let mmap_old = Mmap::map(&path)?;
        let mut mmap = MmapMut::map(path)?;

        let diff_file = File::open(diff_path)?;
        let mut decoder = DeflateDecoder::new(diff_file);

        patch(&mmap_old, &mut decoder, &mut mmap)?;

        Ok(Self {
            inner: Arc::new(RwLock::new(MemoryInner {
                mmap,
                init_count: INIT_COUNT,
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
    inner: RwLockReadGuard<'a, MemoryInner>,
}

impl<'a> AsRef<[u8]> for MemoryReadGuard<'a> {
    fn as_ref(&self) -> &[u8] {
        &self.inner.mmap
    }
}

impl<'a> Deref for MemoryReadGuard<'a> {
    type Target = MmapMut;

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
    type Target = MmapMut;

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
        let mut inner = self.inner.write().unwrap();

        match inner.init_count == INIT_COUNT {
            true => Ok(()),
            false => {
                let memory = inner.mmap.vmmemory().as_ref();
                initialize_memory_with_data(memory, start, data).map(|_| {
                    inner.init_count += 1;
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
