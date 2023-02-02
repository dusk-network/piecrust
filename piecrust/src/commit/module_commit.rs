// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

// use qbsdiff::{Bsdiff, Bspatch};
use bsdiff::diff::diff;
use bsdiff::patch::patch;
use std::fs::OpenOptions;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use zstd::block::Decompressor;

use crate::commit::diff_data::DiffData;
use crate::commit::{Hashable, ModuleCommitId};
use crate::memory_path::MemoryPath;
use crate::util::ByteArrayWrapper;
use crate::Error::{self, PersistenceError, PersistenceError1, PersistenceError2, PersistenceError3, PersistenceError4, PersistenceError5, PersistenceError6, PersistenceError7};
use std::time::{Duration, Instant};


pub trait ModuleCommitLike {
    /// Module commit's file path
    fn path(&self) -> &PathBuf;
    /// Read's module commit' content into buffer
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

#[derive(Debug)]
pub struct ModuleCommit {
    path: PathBuf,
    id: ModuleCommitId,
    pub patch_duration: Duration,
}

fn combine_module_commit_names(
    module_name: impl AsRef<str>,
    commit_name: impl AsRef<str>,
) -> String {
    format!("{}_{}", module_name.as_ref(), commit_name.as_ref())
}

fn module_commit_id_to_name(module_commit_id: ModuleCommitId) -> String {
    format!("{}", ByteArrayWrapper(module_commit_id.as_slice()))
}

const COMPRESSION_LEVEL: i32 = 11;

impl ModuleCommit {
    /// Creates module commit with a given module commit id.
    /// Filename for module commit is a concatenation of
    /// a given path filename and a given module commit id.
    pub(crate) fn from_id_and_path(
        module_commit_id: ModuleCommitId,
        path: &PathBuf,
    ) -> Result<Self, Error> {
        let mut path = path.to_owned();
        path.set_file_name(combine_module_commit_names(
            path.file_name()
                .expect("filename exists")
                .to_str()
                .expect("filename is UTF8"),
            module_commit_id_to_name(module_commit_id),
        ));
        Ok(ModuleCommit {
            path,
            id: module_commit_id,
            patch_duration: Duration::from_millis(0),
        })
    }

    pub(crate) fn from_id_and_path_direct(
        module_commit_id: ModuleCommitId,
        path: &Path,
    ) -> Result<Self, Error> {
        Ok(ModuleCommit {
            path: path.to_path_buf(),
            id: module_commit_id,
            patch_duration: Duration::from_millis(0),
        })
    }

    /// Captures contents of a given module commit into 'this' module
    /// commit.
    pub(crate) fn capture(
        &self,
        commit: &dyn ModuleCommitLike,
    ) -> Result<(), Error> {
        std::fs::copy(commit.path(), self.path().as_path())
            .map_err(PersistenceError7)?;
        Ok(())
    }

    /// Restores contents of 'this' module commit into current memory.
    pub(crate) fn restore(
        &self,
        memory_path: &MemoryPath,
    ) -> Result<(), Error> {
        std::fs::copy(self.path().as_path(), memory_path.path())
            .map_err(PersistenceError6)?;
        Ok(())
    }

    pub fn id(&self) -> ModuleCommitId {
        self.id
    }

    /// Captured the difference of memory path and the given base module
    /// commit into 'this' module commit.
    pub(crate) fn capture_diff(
        &self,
        base_commit: &ModuleCommit,
        memory_path: &MemoryPath,
    ) -> Result<(), Error> {
        let mut compressor = zstd::block::Compressor::new();
        let memory_buffer = memory_path.read()?;
        let base_buffer = base_commit.read()?;
        fn bsdiff(source: &[u8], target: &[u8]) -> std::io::Result<Vec<u8>> {
            let mut patch = Vec::new();
            // Bsdiff::new(source, target)
            //     .compare(std::io::Cursor::new(&mut patch))?;
            diff(source, target, &mut patch)?;
            Ok(patch)
        }
        let delta = bsdiff(base_buffer.as_slice(), memory_buffer.as_slice())
            .map_err(PersistenceError)?;
        let diff_data = DiffData::new(
            base_buffer.as_slice().len(),
            compressor.compress(&delta, COMPRESSION_LEVEL)
                .map_err(PersistenceError5)?,
            delta.len(),
        );
        diff_data.persist(self.path())?;
        Ok(())
    }

    /// Decompresses 'this' module commit as patch and patches a given module
    /// commit. Result is written to a result module commit.
    pub(crate) fn decompress_and_patch_last(
        &mut self,
        previous_patched: &[u8],
        result_commit: &dyn ModuleCommitLike,
        decompressor: &mut Decompressor,
    ) -> Result<(), Error> {
        let patched =
            self.decompress_and_patch(previous_patched, decompressor)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(result_commit.path())
            .map_err(PersistenceError4)?;
        file.write_all(patched.as_slice())
            .map_err(PersistenceError3)?;
        Ok(())
    }

    /// Decompresses 'this' module commit as patch and patches a given module
    /// commit. Result is passed back as a return parameter.
    pub(crate) fn decompress_and_patch(
        &mut self,
        previous_patched: &[u8],
        decompressor: &mut Decompressor,
    ) -> Result<Vec<u8>, Error> {
        let diff_data: DiffData = DiffData::restore(self.path())?;
        let mut patch_data = std::io::Cursor::new(
            decompressor
                .decompress(diff_data.data(), diff_data.uncompressed_size)
                .map_err(PersistenceError2)?,
        );
        let patched = self.patch(&mut patch_data, previous_patched, diff_data.original_len())?;
        Ok(patched)
    }

    fn patch(
        &mut self,
        patch_data: &mut Cursor<Vec<u8>>,
        vector_to_patch: &[u8],
        sz: usize
    ) -> Result<Vec<u8>, Error> {
        fn bspatch(slf: &mut ModuleCommit, source: &[u8], patch_data: &mut Cursor<Vec<u8>>, sz: usize) -> std::io::Result<Vec<u8>> {
            let now = Instant::now();
            // let patcher =
            //     Bspatch::new(patch)?.buffer_size(4096).delta_min(1024);
            // let mut target =
            //     Vec::with_capacity(patcher.hint_target_size() as usize);
            // println!("source={} hint={}", source.len(), patcher.hint_target_size());
            // patcher.apply(source, std::io::Cursor::new(&mut target))?;
            let mut target = vec![0u8; sz];
            patch(source, patch_data, target.as_mut_slice())?;
            slf.patch_duration = now.elapsed();
            Ok(target)
        }
        let patched =
            bspatch(self, vector_to_patch, patch_data, sz)
                .map_err(PersistenceError1)?;
        Ok(patched)
    }
}

impl ModuleCommitLike for ModuleCommit {
    fn path(&self) -> &PathBuf {
        &self.path
    }
}
