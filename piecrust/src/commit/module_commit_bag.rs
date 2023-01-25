// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use bytecheck::CheckBytes;
use rkyv::{Archive, Deserialize, Serialize};

use crate::error::Error;
use crate::commit::{ModuleCommit, ModuleCommitId};
use crate::commit::module_commit::ModuleCommitLike;
use crate::memory_path::MemoryPath;
use crate::Error::CommitError;

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
    ) -> Result<(), Error> {
        self.ids.push(module_commit.id());
        if self.ids.len() == 1 {
            // top is an uncompressed version of most recent commit
            self.top = module_commit.id();
            Ok(())
        } else {
            let from_id = |module_commit_id| {
                ModuleCommit::from_id_and_path(module_commit_id, module_commit.path())
            };
            let top_commit = from_id(self.top)?;
            let accu_commit = from_id(ModuleCommitId::random())?;
            accu_commit.capture(module_commit)?;
            // accu commit and module commit are both uncompressed
            // compressing module_commit against the top
            module_commit.capture_diff(&top_commit, &MemoryPath::new(module_commit.path()))?;
            // module commit is compressed but accu keeps the uncompressed copy
            // top commit is an uncompressed version of most recent commit
            top_commit.capture(&accu_commit)?;
            Ok(())
        }
    }

    pub(crate) fn restore_module_commit(
        &self,
        source_module_commit_id: ModuleCommitId,
        memory_path: &MemoryPath,
    ) -> Result<(), Error> {
        let from_id = |module_commit_id| {
            ModuleCommit::from_id_and_path(module_commit_id, memory_path.path())
        };
        let mut found = true;
        let final_commit = if source_module_commit_id == self.ids[0] {
            from_id(self.ids[0])?
        } else if source_module_commit_id == self.top {
            from_id(self.top)?
        } else {
            let accu_commit = from_id(ModuleCommitId::random())?;
            accu_commit.capture(&from_id(self.ids[0])?)?;
            for commit_id in self.ids.as_slice()[1..].iter() {
                let commit = from_id(*commit_id)?;
                commit
                    .decompress_and_patch(&accu_commit, &accu_commit)?;
                found = source_module_commit_id == *commit_id;
                if found {
                    break;
                }
            }
            accu_commit
        };
        final_commit.restore(memory_path)
    }
}
