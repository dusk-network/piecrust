// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::fs;

use bytecheck::CheckBytes;
use rkyv::{Archive, Deserialize, Serialize};

use crate::commit::module_commit::ModuleCommitLike;
use crate::commit::{CommitPath, ModuleCommit, ModuleCommitId};
use crate::error::Error;
use crate::memory_path::MemoryPath;
use crate::Error::{CommitError, PersistenceError};

#[derive(Debug)]
pub struct BagSizeInfo {
    commit_sizes: Vec<u64>,
    top_commit_size: u64,
}

impl BagSizeInfo {
    pub fn new() -> Self {
        Self {
            commit_sizes: Vec::new(),
            top_commit_size: 0u64,
        }
    }

    pub fn commit_sizes(&self) -> &Vec<u64> {
        &self.commit_sizes
    }

    pub fn top_commit_size(&self) -> u64 {
        self.top_commit_size
    }
}

impl Default for BagSizeInfo {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Archive, Serialize, Deserialize)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct ModuleCommitBag {
    // first module commit is always uncompressed
    ids: Vec<ModuleCommitId>,
    // we keep top uncompressed module commit
    // to make saving module commits efficient
    top: ModuleCommitId,
}

impl ModuleCommitBag {
    pub fn new() -> Self {
        Self {
            ids: Vec::new(),
            top: ModuleCommitId::random(),
        }
    }

    pub(crate) fn save_module_commit(
        &mut self,
        module_commit: &ModuleCommit,
        memory_path: &MemoryPath,
    ) -> Result<(), Error> {
        module_commit.capture(memory_path)?;
        self.ids.push(module_commit.id());
        if self.ids.len() == 1 {
            // top is an uncompressed version of most recent commit
            ModuleCommit::from_id_and_path(self.top, memory_path.path())?
                .capture(memory_path)?;
            Ok(())
        } else {
            let from_id = |module_commit_id| {
                ModuleCommit::from_id_and_path(
                    module_commit_id,
                    memory_path.path(),
                )
            };
            let top_commit = from_id(self.top)?;
            let accu_commit = from_id(ModuleCommitId::random())?;
            accu_commit.capture(module_commit)?;
            // accu and commit are both uncompressed
            // compressing commit against the top
            module_commit.capture_diff(&top_commit, memory_path)?;
            // commit is compressed but accu keeps the uncompressed copy
            // top is an uncompressed version of most recent commit
            top_commit.capture(&accu_commit)?;
            fs::remove_file(accu_commit.path()).map_err(PersistenceError)?;
            Ok(())
        }
    }

    pub(crate) fn restore_module_commit(
        &self,
        source_module_commit_id: ModuleCommitId,
        memory_path: &MemoryPath,
    ) -> Result<Option<CommitPath>, Error> {
        if self.ids.is_empty() {
            return Ok(None);
        }
        let from_id = |module_commit_id| {
            ModuleCommit::from_id_and_path(module_commit_id, memory_path.path())
        };
        let mut found = true;
        let mut can_remove = false;
        let final_commit = if source_module_commit_id == self.ids[0] {
            from_id(self.ids[0])?
        } else if source_module_commit_id == self.top {
            from_id(self.top)?
        } else {
            let accu_commit = from_id(ModuleCommitId::random())?;
            accu_commit.capture(&from_id(self.ids[0])?)?;
            let mut previous_patched: Vec<u8> = Vec::<u8>::new();
            let mut decompressor = zstd::block::Decompressor::new();
            for (i, commit_id) in self.ids.as_slice()[1..].iter().enumerate() {
                let is_first = i == 0;
                let is_last = (i + 2) == (self.ids.len());
                let commit = from_id(*commit_id)?;
                if is_first {
                    previous_patched = accu_commit.read()?;
                }
                if is_last {
                    commit.decompress_and_patch_last(
                        previous_patched.as_slice(),
                        &accu_commit,
                        &mut decompressor,
                    )?;
                } else {
                    previous_patched = commit.decompress_and_patch(
                        previous_patched.as_slice(),
                        &mut decompressor,
                    )?;
                }
                found = source_module_commit_id == *commit_id;
                if found {
                    break;
                }
            }
            can_remove = true;
            accu_commit
        };
        if found {
            Ok(Some(CommitPath::new(final_commit.path(), can_remove)))
        } else {
            Err(CommitError("Commit id not found".into()))
        }
    }

    pub(crate) fn get_bag_size_info(
        &self,
        memory_path: &MemoryPath,
    ) -> Result<BagSizeInfo, Error> {
        fn get_size(
            id: &ModuleCommitId,
            memory_path: &MemoryPath,
        ) -> Result<u64, Error> {
            let module_commit =
                ModuleCommit::from_id_and_path(*id, memory_path.path())?;
            let metadata = std::fs::metadata(module_commit.path())
                .expect("metadata obtained");
            Ok(metadata.len())
        }
        let mut bag_size_info = BagSizeInfo::new();
        for id in self.ids.iter() {
            bag_size_info.commit_sizes.push(get_size(id, memory_path)?);
        }
        bag_size_info.top_commit_size = get_size(&self.top, memory_path)?;
        Ok(bag_size_info)
    }
}

impl Default for ModuleCommitBag {
    fn default() -> Self {
        Self::new()
    }
}
