use std::fs::OpenOptions;
use std::io;
use std::path::Path;
use std::sync::Arc;

use memmap2::{MmapMut, MmapOptions};
use parking_lot::{ReentrantMutex, ReentrantMutexGuard};

const PAGE_SIZE: usize = 65536;
const MINIMUM_PAGES: usize = 4;

/// WASM memory belonging to a given module during a given session.
#[derive(Debug, Clone)]
pub struct Memory {
    mmap: Arc<ReentrantMutex<MmapMut>>,
}

impl Memory {
    pub(crate) fn new() -> io::Result<Self> {
        let mmap = MmapMut::map_anon(MINIMUM_PAGES * PAGE_SIZE)?;
        Ok(Self {
            mmap: Arc::new(ReentrantMutex::new(mmap)),
        })
    }

    pub(crate) fn from_file<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = OpenOptions::new().read(true).write(true).open(&path)?;
        // SAFETY: memory files will be opened with write permissions, but only
        // for the purpose of creating this mmap. If any other process mutates
        // the file in any way, the code will break.
        let mmap = unsafe { MmapOptions::new().map_copy(&file)? };
        Ok(Self {
            mmap: Arc::new(ReentrantMutex::new(mmap)),
        })
    }

    pub fn lock(&self) -> MemoryGuard {
        let mmap = self.mmap.lock();
        MemoryGuard { mmap }
    }
}

pub struct MemoryGuard<'a> {
    mmap: ReentrantMutexGuard<'a, MmapMut>,
}

impl<'a> AsRef<[u8]> for MemoryGuard<'a> {
    fn as_ref(&self) -> &[u8] {
        &self.mmap
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
            todo!()
        }

        fn size(&self) -> Pages {
            todo!()
        }

        fn style(&self) -> MemoryStyle {
            todo!()
        }

        fn grow(&mut self, _delta: Pages) -> Result<Pages, MemoryError> {
            todo!()
        }

        fn vmmemory(&self) -> NonNull<VMMemoryDefinition> {
            todo!()
        }

        fn try_clone(&self) -> Option<Box<dyn LinearMemory + 'static>> {
            todo!()
        }
    }
}
