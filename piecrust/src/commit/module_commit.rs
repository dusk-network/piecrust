// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::path::PathBuf;
use crate::commit::{Hashable, ModuleCommitId};
use crate::memory_path::MemoryPath;
use crate::Error::{self, PersistenceError};
use crate::util::ByteArrayWrapper;

pub trait ModuleCommitLike {
    fn path(&self) -> &PathBuf;
}

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

impl ModuleCommit {
    /// Creates module commit with a given module commit id.
    pub(crate) fn from_id(
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
        })
    }

    /// Captures contents of a given module commit  into 'this' module
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
        _base_commit: &ModuleCommit,
        _memory_path: &MemoryPath,
    ) -> Result<(), Error> {
        Ok(())
    }

    /// Decompresses 'this' module commit as patch and patches a given module
    /// commit. Result is written to a result module commit.
    pub(crate) fn decompress_and_patch(
        &self,
        _commit_to_patch: &ModuleCommit,
        _result_commit: &dyn ModuleCommitLike,
    ) -> Result<(), Error> {
        Ok(())
    }
}

impl ModuleCommitLike for ModuleCommit {
    fn path(&self) -> &PathBuf {
        &self.path
    }
}
