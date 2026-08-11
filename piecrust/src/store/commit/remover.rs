// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::path::Path;
use std::{fs, io};

use crate::store::baseinfo::BaseInfo;
use crate::store::commit::operation::CommitOperation;
use crate::store::hasher::Hash;
use crate::store::{BASE_FILE, LEAF_DIR, MAIN_DIR, MEMORY_DIR};

pub struct CommitRemover;

fn remove_dir_all_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

impl CommitRemover {
    /// Delete the given commit's directory.
    pub fn remove<P: AsRef<Path>>(root_dir: P, root: Hash) -> io::Result<()> {
        let root_dir = root_dir.as_ref();
        match CommitOperation::pending(root_dir, root)? {
            Some(CommitOperation::Delete) => {}
            Some(CommitOperation::Finalize) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Commit already has a pending finalize operation",
                ));
            }
            None => {
                CommitOperation::begin(
                    root_dir,
                    root,
                    CommitOperation::Delete,
                )?;
            }
        }

        Self::resume(root_dir, root)
    }

    pub fn resume<P: AsRef<Path>>(root_dir: P, root: Hash) -> io::Result<()> {
        let root_dir = root_dir.as_ref();
        if CommitOperation::pending(root_dir, root)?
            != Some(CommitOperation::Delete)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Commit has no pending delete operation",
            ));
        }

        let root_hex = hex::encode(root);
        let root_main_dir = root_dir.join(MAIN_DIR);
        let commit_dir = root_main_dir.join(&root_hex);
        let base_info_path = commit_dir.join(BASE_FILE);
        let base_info = BaseInfo::from_path(base_info_path)?;
        for contract_hint in base_info.contract_hints {
            let contract_hex = hex::encode(contract_hint);
            let commit_mem_path = root_main_dir
                .join(MEMORY_DIR)
                .join(&contract_hex)
                .join(&root_hex);
            remove_dir_all_if_exists(&commit_mem_path)?;
            let commit_leaf_path = root_main_dir
                .join(LEAF_DIR)
                .join(&contract_hex)
                .join(&root_hex);
            remove_dir_all_if_exists(&commit_leaf_path)?;
        }

        CommitOperation::complete(root_dir, root, CommitOperation::Delete)
    }
}
