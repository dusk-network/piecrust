use std::io;
use std::path::Path;
use std::sync::Arc;

use crate::store::mmap::Mmap;

/// WASM bytecode belonging to a given module.
#[derive(Debug, Clone)]
pub struct Bytecode {
    mmap: Arc<Mmap>,
}

impl Bytecode {
    pub(crate) fn new<B: AsRef<[u8]>>(bytes: B) -> io::Result<Self> {
        let mmap = Mmap::new(bytes)?;

        Ok(Self {
            mmap: Arc::new(mmap),
        })
    }

    pub(crate) fn from_file<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let mmap = Mmap::map(path)?;
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
