
use dallo::SnapshotId;
use std::path::{Path, PathBuf};
use std::io::Read;
use bsdiff::diff::diff;

use crate::error::Error;
use crate::Error::PersistenceError;
use crate::storage_helpers::{
    combine_module_snapshot_names, snapshot_id_to_name,
};


pub struct Snapshot {
    snapshot_id: SnapshotId,
    path: PathBuf,
}

impl Snapshot {
    pub fn current(snapshot_id: SnapshotId, path: impl AsRef<Path>) -> Self {
        Snapshot {
            snapshot_id,
            path: path.as_ref().to_path_buf(),
        }
    }
    pub fn new(snapshot_id: SnapshotId, base_snapshot: &Snapshot) -> Self {
        let mut path = base_snapshot.path().to_owned();
        path.set_file_name(combine_module_snapshot_names(
            path
                .file_name()
                .expect("filename exists")
                .to_str()
                .expect("filename is UTF8"),
            snapshot_id_to_name(snapshot_id),
        ));
        Snapshot {
            snapshot_id,
            path
        }
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Create snapshot by plain copying - compressed implementation will follow
    pub fn write(&self, other_snapshot: &Snapshot) -> Result<(), Error> {
        std::fs::copy(other_snapshot.path(), self.path().as_path()).map_err(PersistenceError)?;
        Ok(())
    }

    /// Load snapshot
    pub fn load(&self) -> Result<Vec<u8>, Error> {
        let mut f = std::fs::File::open(self.path.as_path()).map_err(PersistenceError)?;
        let metadata = std::fs::metadata(self.path.as_path()).map_err(PersistenceError)?;
        let mut buffer = vec![0; metadata.len() as usize];
        f.read(buffer.as_mut_slice()).map_err(PersistenceError)?;
        Ok(buffer)
    }

    /// Create compressed snapshot
    pub fn write_compressed(&self, other_snapshot: &Snapshot) -> Result<(), Error> {
        self.write(other_snapshot)?; // todo! remove it - for the time being we need both sides for the delta calculation to be present
        let mut compressor = zstd::block::Compressor::new();
        let mut delta: Vec<u8> = Vec::new();
        let this_buffer = self.load()?;
        let other_buffer = other_snapshot.load()?;
        diff(other_buffer.as_slice(), this_buffer.as_slice(), &mut delta).unwrap();
        println!("other path={:?}", other_snapshot.path().as_path());
        println!("this path={:?}", self.path().as_path());
        println!("uncompressed patch len={:?}", delta.len());
        delta = compressor.compress(&delta, 11).unwrap();
        println!("compressed patch len={:?}", delta.len());
        // write delta to path
        Ok(())
    }
}
