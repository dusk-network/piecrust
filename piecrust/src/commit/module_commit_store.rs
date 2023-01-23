// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::path::PathBuf;
use std::fs;

use crate::commit::ModuleCommitId;
use crate::memory_path::MemoryPath;
use crate::Error::{self, PersistenceError, RestoreError};
use crate::util::{commit_id_to_name, module_id_to_name};
use crate::ModuleId;
use crate::persistable::Persistable;

const LAST_COMMIT_POSTFIX: &str = "_last";
const LAST_COMMIT_ID_POSTFIX: &str = "_last_id";

pub struct ModuleCommitStore {
    base_path: PathBuf,
    module_id: ModuleId,
}

impl ModuleCommitStore {
    pub fn new(base_path: PathBuf, module_id: ModuleId) -> Self {
        Self{ base_path, module_id }
    }

    pub fn commit(&self, mem: &[u8]) -> Result<ModuleCommitId, Error> {
        let source_path = self.get_memory_path();
        let module_commit_id = ModuleCommitId::from_hash_of(mem)?;
        let target_path =
            self.path_to_module_commit(&module_commit_id);
        let last_commit_path =
            self.path_to_module_last_commit();
        let last_commit_id_path =
            self.path_to_module_last_commit_id();
        std::fs::copy(source_path.as_ref(), target_path.as_ref())
            .map_err(PersistenceError)?;
        std::fs::copy(source_path.as_ref(), last_commit_path.as_ref())
            .map_err(PersistenceError)?;
        module_commit_id.persist(last_commit_id_path)?;
        fs::remove_file(source_path.as_ref()).map_err(PersistenceError)?;
        Ok(module_commit_id)
    }

    pub fn restore(&self, module_commit_id: &ModuleCommitId) -> Result<(), Error> {
        let source_path =
            self.path_to_module_commit(&module_commit_id);
        let target_path = self.get_memory_path();
        let last_commit_path =
            self.path_to_module_last_commit();
        let last_commit_path_id =
            self.path_to_module_last_commit_id();
        std::fs::copy(source_path.as_ref(), target_path.as_ref())
            .map_err(RestoreError)?;
        std::fs::copy(source_path.as_ref(), last_commit_path.as_ref())
            .map_err(RestoreError)?;
        module_commit_id.persist(last_commit_path_id)?;
        Ok(())
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

    fn path_to_module_last_commit(
        &self,
    ) -> MemoryPath {
        self.path_to_module_with_postfix(&self.module_id, LAST_COMMIT_POSTFIX)
    }

    pub(crate) fn path_to_module_last_commit_id(
        &self,
    ) -> MemoryPath {
        self.path_to_module_with_postfix(&self.module_id, LAST_COMMIT_ID_POSTFIX)
    }

    fn path_to_module_with_postfix<P: AsRef<str>>(
        &self,
        module_id: &ModuleId,
        postfix: P,
    ) -> MemoryPath {
        let mut name = module_id_to_name(*module_id);
        name.push_str(postfix.as_ref());
        let path = self.base_path.join(name);
        MemoryPath::new(path)
    }
}
