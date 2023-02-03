// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::path::PathBuf;

use crate::commit::{ModuleCommit, ModuleCommitId, ModuleCommitLike};
use crate::memory_path::MemoryPath;
use crate::util::{commit_id_to_name, module_id_to_name};
use crate::Error::{self, PersistenceError};
use crate::ModuleId;

pub struct ModuleCommitStore {
    base_path: PathBuf,
    module_id: ModuleId,
}

impl ModuleCommitStore {
    pub fn new(base_path: PathBuf, module_id: ModuleId) -> Self {
        Self {
            base_path,
            module_id,
        }
    }

    pub fn commit(&self, mem: &[u8]) -> Result<ModuleCommit, Error> {
        let source_path = self.get_memory_path();
        let module_commit_id = ModuleCommitId::from_hash_of(mem)?;
        let target_path = self.path_to_module_commit(&module_commit_id);
        std::fs::copy(source_path.as_ref(), target_path.as_ref())
            .map_err(PersistenceError)?;
        let module_commit = ModuleCommit::from_id_and_path_direct(
            module_commit_id,
            target_path.path(),
        )?;
        Ok(module_commit)
    }

    fn get_memory_path(&self) -> MemoryPath {
        MemoryPath::new(self.base_path.join(module_id_to_name(self.module_id)))
    }

    fn path_to_module_commit(
        &self,
        module_commit_id: &ModuleCommitId,
    ) -> MemoryPath {
        const SEPARATOR: char = '_';
        let commit_id_name = &*commit_id_to_name(*module_commit_id);
        let mut name = module_id_to_name(self.module_id);
        name.push(SEPARATOR);
        name.push_str(commit_id_name);
        let path = self.base_path.join(name);
        MemoryPath::new(path)
    }
}
