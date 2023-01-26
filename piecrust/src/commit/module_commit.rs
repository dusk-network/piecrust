// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::path::PathBuf;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use qbsdiff::{Bsdiff, Bspatch};

use crate::commit::{Hashable, ModuleCommitId};
use crate::commit::diff_data::DiffData;
use crate::memory_path::MemoryPath;
use crate::Error::{self, PersistenceError};
use crate::util::ByteArrayWrapper;

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
        println!("xx={:?}", path);
        Ok(ModuleCommit {
            path,
            id: module_commit_id,
        })
    }

    pub(crate) fn from_id_and_path_direct(
        module_commit_id: ModuleCommitId,
        path: &PathBuf,
    ) -> Result<Self, Error> {
        Ok(ModuleCommit {
            path: path.clone(),
            id: module_commit_id,
        })
    }

    /// Captures contents of a given module commit into 'this' module
    /// commit.
    pub(crate) fn capture(
        &self,
        commit: &dyn ModuleCommitLike,
    ) -> Result<(), Error> {
        std::fs::copy(commit.path(), self.path().as_path())
            .map_err(PersistenceError)?;
        Ok(())
    }

    /// Restores contents of 'this' module commit into current memory.
    pub(crate) fn restore(
        &self,
        memory_path: &MemoryPath,
    ) -> Result<(), Error> {
        std::fs::copy(self.path().as_path(), memory_path.path())
            .map_err(PersistenceError)?;
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
            Bsdiff::new(source, target)
                .compare(std::io::Cursor::new(&mut patch))?;
            Ok(patch)
        }
        let delta = bsdiff(base_buffer.as_slice(), memory_buffer.as_slice())
            .map_err(PersistenceError)?;
        let diff_data = DiffData::new(
            base_buffer.as_slice().len(),
            compressor.compress(&delta, COMPRESSION_LEVEL).unwrap(),
        );
        diff_data.write(self.path())?;
        Ok(())
    }

    /// Decompresses 'this' module commit as patch and patches a given module
    /// commit. Result is written to a result module commit.
    pub(crate) fn decompress_and_patch(
        &self,
        commit_to_patch: &ModuleCommit,
        result_commit: &dyn ModuleCommitLike,
    ) -> Result<(), Error> {
        let diff_data = DiffData::read(self.path())?;
        let mut decompressor = zstd::block::Decompressor::new();
        let patch_data = std::io::Cursor::new(
            decompressor
                .decompress(diff_data.data(), diff_data.original_len())
                .map_err(PersistenceError)?,
        );
        fn bspatch(source: &[u8], patch: &[u8]) -> std::io::Result<Vec<u8>> {
            let patcher = Bspatch::new(patch)?;
            let mut target =
                Vec::with_capacity(patcher.hint_target_size() as usize);
            patcher.apply(source, std::io::Cursor::new(&mut target))?;
            Ok(target)
        }
        let patched = bspatch(
            commit_to_patch.read()?.as_slice(),
            patch_data.into_inner().as_slice(),
        )
            .map_err(PersistenceError)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(result_commit.path())
            .map_err(PersistenceError)?;
        file.write_all(patched.as_slice())
            .map_err(PersistenceError)?;
        Ok(())
    }
}

impl ModuleCommitLike for ModuleCommit {
    fn path(&self) -> &PathBuf {
        &self.path
    }
}
