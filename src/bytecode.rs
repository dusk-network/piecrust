use std::fs::File;
use std::io;
use std::path::Path;
use std::sync::Arc;

use memmap2::{Mmap, MmapMut};

/// WASM bytecode belonging to a given module.
#[derive(Debug, Clone)]
pub struct Bytecode {
    mmap: Arc<Mmap>,
}

impl Bytecode {
    pub(crate) fn new<B: AsRef<[u8]>>(bytes: B) -> io::Result<Self> {
        let bytes = bytes.as_ref();

        let mut mmap = MmapMut::map_anon(bytes.len())?;
        mmap.copy_from_slice(bytes);
        let mmap = mmap.make_read_only()?;

        Ok(Self {
            mmap: Arc::new(mmap),
        })
    }

    pub(crate) fn from_file<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = File::open(path)?;
        // SAFETY: bytecode will only ever be opened read-only, so this is
        // considered safe. If any other process mutates the file in any way
        // while this mmap is held, the code will break.
        let mmap = unsafe { Mmap::map(&file)? };
        Ok(Self {
            mmap: Arc::new(mmap),
        })
    }
}

impl AsRef<[u8]> for Bytecode {
    fn as_ref(&self) -> &[u8] {
        &self.mmap
    }
}
