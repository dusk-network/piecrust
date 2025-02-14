// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use crate::store::baseinfo::BaseInfo;
use crate::store::tree::Hash;
use crate::store::{BASE_FILE, LEAF_DIR, MAIN_DIR, MEMORY_DIR};
use std::path::Path;
use std::{fs, io};

pub struct CommitRemover;

impl CommitRemover {
    /// Delete the given commit's directory.
    pub fn remove<P: AsRef<Path>>(root_dir: P, root: Hash) -> io::Result<()> {
        let root = hex::encode(root);
        let root_main_dir = root_dir.as_ref().join(MAIN_DIR);
        let commit_dir = root_main_dir.join(&root);
        if commit_dir.exists() {
            let base_info_path = commit_dir.join(BASE_FILE);
            let base_info = BaseInfo::from_path(base_info_path)?;
            for contract_hint in base_info.contract_hints {
                let contract_hex = hex::encode(contract_hint);
                let commit_mem_path = root_main_dir
                    .join(MEMORY_DIR)
                    .join(&contract_hex)
                    .join(&root);
                fs::remove_dir_all(&commit_mem_path)?;
                let commit_leaf_path = root_main_dir
                    .join(LEAF_DIR)
                    .join(&contract_hex)
                    .join(&root);
                fs::remove_dir_all(&commit_leaf_path)?;
            }
            fs::remove_dir_all(&commit_dir)?;
        }
        Ok(())
    }
}
