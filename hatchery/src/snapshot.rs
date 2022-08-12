// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use crate::error::Error;
use crate::storage_helpers::{
    combine_module_snapshot_names, snapshot_id_to_name,
};
use crate::Error::PersistenceError;
use qbsdiff::Bsdiff;
use qbsdiff::Bspatch;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use rand::Rng;
use rkyv::{Archive, Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::mem;
use std::path::{Path, PathBuf};

const COMPRESSION_LEVEL: i32 = 11;
pub const SNAPSHOT_ID_BYTES: usize = 32;
#[derive(
    Debug,
    PartialEq,
    Eq,
    Archive,
    Serialize,
    Deserialize,
    PartialOrd,
    Ord,
    Hash,
    Clone,
    Copy,
)]
pub struct SnapshotId([u8; SNAPSHOT_ID_BYTES]);
impl SnapshotId {
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
    pub fn random() -> SnapshotId {
        SnapshotId(rand::thread_rng().gen::<[u8; SNAPSHOT_ID_BYTES]>())
    }
}
impl From<[u8; 32]> for SnapshotId {
    fn from(array: [u8; 32]) -> Self {
        SnapshotId(array)
    }
}

pub trait SnapshotLike {
    fn path(&self) -> &PathBuf;
    /// Read's snapshot's content into buffer
    fn read(&self) -> Result<Vec<u8>, Error> {
        let mut f = std::fs::File::open(self.path().as_path())
            .map_err(PersistenceError)?;
        let metadata = std::fs::metadata(self.path().as_path())
            .map_err(PersistenceError)?;
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
        let snapshot_id: SnapshotId = SnapshotId::from(
            *blake3::hash(memory_path.read()?.as_slice()).as_bytes(),
        );
        Snapshot::from_id(snapshot_id, memory_path)
    }

    /// Creates snapshot with a given snapshot id.
    /// Memory path is only used as path pattern,
    /// no contents are captured.
    pub fn from_id(
        snapshot_id: SnapshotId,
        memory_path: &MemoryPath,
    ) -> Result<Self, Error> {
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
            id: snapshot_id,
        })
    }

    /// Captures contents of a given snapshot into 'this' snapshot.
    pub fn capture(&self, snapshot: &dyn SnapshotLike) -> Result<(), Error> {
        std::fs::copy(snapshot.path(), self.path().as_path())
            .map_err(PersistenceError)?;
        Ok(())
    }

    /// Restores contents of 'this' snapshot into current memory.
    pub fn restore(&self, memory_path: &MemoryPath) -> Result<(), Error> {
        std::fs::copy(self.path().as_path(), memory_path.path())
            .map_err(PersistenceError)?;
        Ok(())
    }

    /// Captured the difference of memory path and the given base snapshot
    /// into 'this' snapshot.
    pub fn capture_diff(
        &self,
        base_snapshot: &Snapshot,
        memory_path: &MemoryPath,
    ) -> Result<(), Error> {
        let mut compressor = zstd::block::Compressor::new();
        let memory_buffer = memory_path.read()?;
        let base_buffer = base_snapshot.read()?;
        fn bsdiff(source: &[u8], target: &[u8]) -> std::io::Result<Vec<u8>> {
            let mut patch = Vec::new();
            Bsdiff::new(source, target)
                .compare(std::io::Cursor::new(&mut patch))?;
            Ok(patch)
        }
        let delta = bsdiff(base_buffer.as_slice(), memory_buffer.as_slice()).map_err(PersistenceError)?;
        let compressed_delta =
            compressor.compress(&delta, COMPRESSION_LEVEL).unwrap();
        self.write_compressed(
            compressed_delta,
            base_buffer.as_slice().len(),
        )?;
        Ok(())
    }

    /// Writes uncompressed size, original length and data to file
    /// associated with 'this' snapshot.
    fn write_compressed(
        &self,
        data: Vec<u8>,
        original_len: usize,
    ) -> Result<(), Error> {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(self.path())
            .map_err(PersistenceError)?;
        file.write_u32::<LittleEndian>(original_len as u32)
            .map_err(PersistenceError)?;
        file.write_all(data.as_slice()).map_err(PersistenceError)?;
        Ok(())
    }

    /// Decompresses 'this' snapshot as patch and patches a given snapshot.
    /// Result is written to a result snapshot.
    pub fn decompress_and_patch(
        &self,
        snapshot_to_patch: &Snapshot,
        result_snapshot: &dyn SnapshotLike,
    ) -> Result<(), Error> {
        let (original_len, compressed) =
            self.read_compressed()?;
        let mut decompressor = zstd::block::Decompressor::new();
        let patch_data = std::io::Cursor::new(
            decompressor
                .decompress(compressed.as_slice(), original_len)
                .map_err(PersistenceError)?,
        );
        fn bspatch(source: &[u8], patch: &[u8]) -> std::io::Result<Vec<u8>> {
            let patcher = Bspatch::new(patch)?;
            let mut target = Vec::with_capacity(patcher.hint_target_size() as usize);
            patcher.apply(source, std::io::Cursor::new(&mut target))?;
            Ok(target)
        }
        let patched = bspatch(
            snapshot_to_patch.read()?.as_slice(),
            patch_data.into_inner().as_slice(),
        ).map_err(PersistenceError)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(result_snapshot.path())
            .map_err(PersistenceError)?;
        file.write_all(patched.as_slice())
            .map_err(PersistenceError)?;
        Ok(())
    }

    /// Reads uncompressed size, original length and data from file
    /// associated with 'this' snapshot.
    fn read_compressed(&self) -> Result<(usize, Vec<u8>), Error> {
        let mut file = std::fs::File::open(self.path().as_path())
            .map_err(PersistenceError)?;
        let metadata = std::fs::metadata(self.path().as_path())
            .map_err(PersistenceError)?;
        const SIZES_LEN: usize = mem::size_of::<u32>() as usize;
        let mut data = vec![0; metadata.len() as usize - SIZES_LEN];
        let size = file.read_u32::<LittleEndian>().map_err(PersistenceError)?;
        file.read(data.as_mut_slice()).map_err(PersistenceError)?;
        Ok((size as usize, data))
    }

    pub fn id(&self) -> SnapshotId {
        self.id
    }
}

impl SnapshotLike for Snapshot {
    fn path(&self) -> &PathBuf {
        &self.path
    }
}
