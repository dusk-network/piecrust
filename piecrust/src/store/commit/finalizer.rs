// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::path::Path;
use std::{fs, io};

use crate::store::baseinfo::BaseInfo;
use crate::store::commit::Commit;
use crate::store::commit::operation::CommitOperation;
use crate::store::hasher::Hash;
use crate::store::{BASE_FILE, ELEMENT_FILE, LEAF_DIR, MAIN_DIR, MEMORY_DIR};

pub struct CommitFinalizer;

fn read_dir_if_exists(path: &Path) -> io::Result<Option<fs::ReadDir>> {
    match fs::read_dir(path) {
        Ok(entries) => Ok(Some(entries)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn remove_dir_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_dir(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

impl CommitFinalizer {
    pub fn finalize<P: AsRef<Path>>(
        root: Hash,
        root_dir: P,
        commit: &Commit,
    ) -> io::Result<()> {
        let root_dir = root_dir.as_ref();
        match CommitOperation::pending(root_dir, root)? {
            Some(CommitOperation::Finalize) => {}
            Some(CommitOperation::Delete) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Commit already has a pending delete operation",
                ));
            }
            None => {
                commit.validate_persisted_state(root_dir, root)?;
                CommitOperation::begin(
                    root_dir,
                    root,
                    CommitOperation::Finalize,
                )?;
            }
        }

        Self::resume(root, root_dir)
    }

    pub fn resume<P: AsRef<Path>>(root: Hash, root_dir: P) -> io::Result<()> {
        let root_dir = root_dir.as_ref();
        if CommitOperation::pending(root_dir, root)?
            != Some(CommitOperation::Finalize)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Commit has no pending finalize operation",
            ));
        }

        let main_dir = root_dir.join(MAIN_DIR);
        let root_hex = hex::encode(root);
        let commit_path = main_dir.join(&root_hex);
        let base_info_path = commit_path.join(BASE_FILE);
        let base_info = BaseInfo::from_path(&base_info_path)?;
        for contract_hint in base_info.contract_hints {
            let contract_hex = hex::encode(contract_hint);
            // MEMORY
            let src_path = main_dir
                .join(MEMORY_DIR)
                .join(&contract_hex)
                .join(&root_hex);
            let dst_path = main_dir.join(MEMORY_DIR).join(&contract_hex);
            if let Some(entries) = read_dir_if_exists(&src_path)? {
                for entry in entries {
                    let filename =
                        entry?.file_name().to_string_lossy().to_string();
                    let src_file_path = src_path.join(&filename);
                    let dst_file_path = dst_path.join(&filename);
                    if src_file_path.is_file() {
                        fs::rename(&src_file_path, dst_file_path)?;
                    }
                }
                remove_dir_if_exists(&src_path)?;
            }
            // LEAF
            let src_leaf_path =
                main_dir.join(LEAF_DIR).join(&contract_hex).join(&root_hex);
            let dst_leaf_path = main_dir.join(LEAF_DIR).join(&contract_hex);
            let src_leaf_file_path = src_leaf_path.join(ELEMENT_FILE);
            let dst_leaf_file_path = dst_leaf_path.join(ELEMENT_FILE);
            if src_leaf_file_path.is_file() {
                fs::rename(&src_leaf_file_path, dst_leaf_file_path)?;
            }
            if src_leaf_path.exists() {
                fs::remove_dir(src_leaf_path)?;
            }
        }

        CommitOperation::complete(root_dir, root, CommitOperation::Finalize)
    }
}
