use std::fs::File;
use std::io;
use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use flate2::read::DeflateDecoder;

use crate::mmap::{Mmap, MmapMut};

/// WASM memory belonging to a given module during a given session.
#[derive(Debug, Clone)]
pub struct Memory {
    mmap: Arc<RwLock<MmapMut>>,
}

impl Memory {
    pub(crate) fn new() -> io::Result<Self> {
        let mmap = MmapMut::new()?;
        Ok(Self {
            mmap: Arc::new(RwLock::new(mmap)),
        })
    }

    pub(crate) fn from_file<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let mmap = MmapMut::map(path)?;
        Ok(Self {
            mmap: Arc::new(RwLock::new(mmap)),
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

        bsdiff::patch::patch(&mmap_old, &mut decoder, &mut mmap)?;

        Ok(Self {
            mmap: Arc::new(RwLock::new(mmap)),
        })
    }

    pub fn read(&self) -> MemoryReadGuard {
        let mmap = self.mmap.read().unwrap();
        MemoryReadGuard { mmap }
    }

    pub fn write(&self) -> MemoryWriteGuard {
        let mmap = self.mmap.write().unwrap();
        MemoryWriteGuard { mmap }
    }
}

pub struct MemoryReadGuard<'a> {
    mmap: RwLockReadGuard<'a, MmapMut>,
}

impl<'a> AsRef<[u8]> for MemoryReadGuard<'a> {
    fn as_ref(&self) -> &[u8] {
        &self.mmap
    }
}

impl<'a> Deref for MemoryReadGuard<'a> {
    type Target = MmapMut;

    fn deref(&self) -> &Self::Target {
        &self.mmap
    }
}

pub struct MemoryWriteGuard<'a> {
    mmap: RwLockWriteGuard<'a, MmapMut>,
}

impl<'a> AsRef<[u8]> for MemoryWriteGuard<'a> {
    fn as_ref(&self) -> &[u8] {
        &self.mmap
    }
}

impl<'a> AsMut<[u8]> for MemoryWriteGuard<'a> {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.mmap
    }
}

impl<'a> Deref for MemoryWriteGuard<'a> {
    type Target = MmapMut;

    fn deref(&self) -> &Self::Target {
        &self.mmap
    }
}

impl<'a> DerefMut for MemoryWriteGuard<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.mmap
    }
}

#[cfg(feature = "wasmer")]
mod wasmer {
    use std::ptr::NonNull;

    use wasmer_types::{MemoryType, Pages};
    use wasmer_vm::{
        LinearMemory, MemoryError, MemoryStyle, VMMemoryDefinition,
    };

    use super::*;

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
            self.write().vmmemory()
        }

        fn try_clone(&self) -> Option<Box<dyn LinearMemory + 'static>> {
            self.read().try_clone()
        }
    }
}
