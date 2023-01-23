// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use crate::error::Error;
use crate::commit::{ModuleCommit, ModuleCommitId};
use crate::commit::module_commit::ModuleCommitLike;
use crate::memory_path::MemoryPath;
use crate::Error::CommitError;

#[derive(Debug)]
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
    ) -> Result<usize, Error> {
        module_commit.capture(memory_path)?;
        self.ids.push(module_commit.id());
        if self.ids.len() == 1 {
            // top is an uncompressed version of most recent commit
            ModuleCommit::from_id_and_path(self.top, memory_path.path())?
                .capture(memory_path)?;
            Ok(0)
        } else {
            let from_id = |module_commit_id| {
                ModuleCommit::from_id_and_path(module_commit_id, memory_path.path())
            };
            let top_commit = from_id(self.top)?;
            let accu_commit = from_id(ModuleCommitId::random())?;
            accu_commit.capture(module_commit)?;
            // accu commit and module commit are both uncompressed
            // compressing module_commit against the top
            module_commit.capture_diff(&top_commit, memory_path)?;
            // module commit is compressed but accu keeps the uncompressed copy
            // top commit is an uncompressed version of most recent commit
            top_commit.capture(&accu_commit)?;
            Ok(self.ids.len() - 1)
        }
    }

    pub(crate) fn restore_module_commit(
        &self,
        module_commit_index: usize,
        memory_path: &MemoryPath,
    ) -> Result<(), Error> {
        let is_valid = |index| index < self.ids.len();
        if !is_valid(module_commit_index) {
            return Err(CommitError("invalid commit index".into()));
        }
        let is_top = |index| (index + 1) == self.ids.len();
        let from_id = |module_commit_id| {
            ModuleCommit::from_id_and_path(module_commit_id, memory_path.path())
        };
        let final_commit = if module_commit_index == 0 {
            from_id(self.ids[0])?
        } else if is_top(module_commit_index) {
            from_id(self.top)?
        } else {
            let accu_commit = from_id(ModuleCommitId::random())?;
            accu_commit.capture(&from_id(self.ids[0])?)?;
            for i in 1..(module_commit_index + 1) {
                let commit = from_id(self.ids[i])?;
                commit
                    .decompress_and_patch(&accu_commit, &accu_commit)?;
            }
            accu_commit
        };
        final_commit.restore(memory_path)
    }
}
