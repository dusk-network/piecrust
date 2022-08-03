use bsdiff::diff::diff;
use bsdiff::patch::patch;
use dallo::SnapshotId;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::mem;
use std::path::{Path, PathBuf};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use crate::error::Error;
use crate::storage_helpers::{
    combine_module_snapshot_names, snapshot_id_to_name,
};
use crate::Error::PersistenceError;

const COMPRESSION_LEVEL: i32 = 11;

pub trait SnapshotLike {
    fn path(&self) -> &PathBuf;
    /// Load snapshot as buffer
    fn load(&self) -> Result<Vec<u8>, Error> {
        let mut f = std::fs::File::open(self.path().as_path())
            .map_err(PersistenceError)?;
        let metadata =
            std::fs::metadata(self.path().as_path()).map_err(PersistenceError)?;
        let mut buffer = vec![0; metadata.len() as usize];
        f.read(buffer.as_mut_slice()).map_err(PersistenceError)?;
        Ok(buffer)
    }
}

pub struct MemoryPath {
    path: PathBuf,
}

impl MemoryPath {
    pub fn new(path: impl AsRef<Path>) -> Self {
        MemoryPath {
            path: path.as_ref().to_path_buf(),
        }
    }
}

impl SnapshotLike for MemoryPath {
    fn path(&self) -> &PathBuf {
        &self.path
    }
}

pub struct Snapshot {
    path: PathBuf,
    id: SnapshotId,
}

impl Snapshot {
    pub fn new(memory_path: &MemoryPath) -> Result<Self, Error> {
        let snapshot_id: SnapshotId = blake3::hash(memory_path.load()?.as_slice()).into();
        Snapshot::from_id(snapshot_id, memory_path)
    }

    pub fn from_id(snapshot_id: SnapshotId, memory_path: &MemoryPath) -> Result<Self, Error> {
        let mut path = memory_path.path().to_owned();
        path.set_file_name(combine_module_snapshot_names(
            path.file_name()
                .expect("filename exists")
                .to_str()
                .expect("filename is UTF8"),
            snapshot_id_to_name(snapshot_id),
        ));
        Ok(Snapshot {
            path,
            id: snapshot_id
        })
    }

    pub fn from_buffer(buf: Vec<u8>, memory_path: &MemoryPath) -> Result<Self, Error> {
        let snapshot_id: SnapshotId = blake3::hash(buf.as_slice()).into();
        let snapshot = Snapshot::from_id(snapshot_id, memory_path)?;
        snapshot.save(buf)?;
        Ok(snapshot)
    }

    fn save(&self, buf: Vec<u8>) -> Result<(), Error> {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(self.path())
            .map_err(PersistenceError)?;
        file.write_all(buf.as_slice()).map_err(PersistenceError)?;
        Ok(())
    }

    /// Save current snapshot as uncompressed
    pub fn write_uncompressed(&self, memory_path: &MemoryPath) -> Result<(), Error> {
        std::fs::copy(memory_path.path(), self.path().as_path())
            .map_err(PersistenceError)?;
        Ok(())
    }

    /// Save current snapshot as compressed
    pub fn write_compressed(
        &self,
        diff1: &MemoryPath,
        diff2: &Snapshot,
    ) -> Result<(), Error> {
        let mut compressor = zstd::block::Compressor::new();
        let mut delta: Vec<u8> = Vec::new();
        let diff1_buffer = diff1.load()?;
        let diff2_buffer = diff2.load()?;
        diff(diff2_buffer.as_slice(), diff1_buffer.as_slice(), &mut delta).unwrap();
        let compressed_delta = compressor.compress(&delta, COMPRESSION_LEVEL).unwrap();
        self.save_with_sizes(compressed_delta, delta.len(), diff2_buffer.as_slice().len())?;
        Ok(())
    }

    /// Decompress current snapshot into the given snapshot
    pub fn decompress(
        &self,
        old_snapshot: &Snapshot,
        memory_path: &MemoryPath,
    ) -> Result<Snapshot, Error> {
        let (original_size, old_size, compressed) = self.load_with_sizes()?;
        let old = old_snapshot.load()?;
        let mut decompressor = zstd::block::Decompressor::new();
        let mut patch_data = std::io::Cursor::new(
            decompressor
                .decompress(compressed.as_slice(), original_size)
                .map_err(PersistenceError)?,
        );
        let mut patched = vec![0; old_size];
        patch(old.as_slice(), &mut patch_data, patched.as_mut_slice())
            .map_err(PersistenceError)?;
        let out_snapshot = Snapshot::from_buffer(patched, memory_path)?;
        Ok(out_snapshot)
    }

    fn save_with_sizes(&self, buf: Vec<u8>, size: usize, original_len: usize) -> Result<(), Error> {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(self.path())
            .map_err(PersistenceError)?;
        file.write_u32::<LittleEndian>(size as u32).map_err(PersistenceError)?;
        file.write_u32::<LittleEndian>(original_len as u32).map_err(PersistenceError)?;
        file.write_all(buf.as_slice()).map_err(PersistenceError)?;
        Ok(())
    }

    fn load_with_sizes(&self) -> Result<(usize, usize, Vec<u8>), Error> {
        let mut f = std::fs::File::open(self.path().as_path())
            .map_err(PersistenceError)?;
        let metadata =
            std::fs::metadata(self.path().as_path()).map_err(PersistenceError)?;
        let mut buffer = vec![0; metadata.len() as usize - (mem::size_of::<u32>() as usize) * 2];
        let size = f.read_u32::<LittleEndian>().map_err(PersistenceError)?;
        let original_len = f.read_u32::<LittleEndian>().map_err(PersistenceError)?;
        f.read(buffer.as_mut_slice()).map_err(PersistenceError)?;
        Ok((size as usize, original_len as usize, buffer))
    }

    /// Id
    pub fn id(&self) -> SnapshotId {
        self.id
    }
}

impl SnapshotLike for Snapshot {
    fn path(&self) -> &PathBuf {
        &self.path
    }
}
