// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use crate::store::baseinfo::BaseInfo;
use crate::store::tree::Hash;
use crate::store::{
    BASE_FILE, ELEMENT_FILE, LEAF_DIR, MAIN_DIR, MEMORY_DIR, TREE_POS_FILE,
    TREE_POS_OPT_FILE,
};
use std::path::Path;
use std::{fs, io};

pub struct CommitFinalizer;

impl CommitFinalizer {
    pub fn finalize<P: AsRef<Path>>(root: Hash, root_dir: P) -> io::Result<()> {
        let main_dir = root_dir.as_ref().join(MAIN_DIR);
        let root = hex::encode(root);
        let commit_path = main_dir.join(&root);
        let base_info_path = commit_path.join(BASE_FILE);
        let tree_pos_path = commit_path.join(TREE_POS_FILE);
        let tree_pos_opt_path = commit_path.join(TREE_POS_OPT_FILE);
        let base_info = BaseInfo::from_path(&base_info_path)?;
        for contract_hint in base_info.contract_hints {
            let contract_hex = hex::encode(contract_hint);
            // MEMORY
            let src_path =
                main_dir.join(MEMORY_DIR).join(&contract_hex).join(&root);
            let dst_path = main_dir.join(MEMORY_DIR).join(&contract_hex);
            for entry in fs::read_dir(&src_path)? {
                let filename = entry?.file_name().to_string_lossy().to_string();
                let src_file_path = src_path.join(&filename);
                let dst_file_path = dst_path.join(&filename);
                if src_file_path.is_file() {
                    fs::rename(&src_file_path, dst_file_path)?;
                }
            }
            fs::remove_dir(&src_path)?;
            // LEAF
            let src_leaf_path =
                main_dir.join(LEAF_DIR).join(&contract_hex).join(&root);
            let dst_leaf_path = main_dir.join(LEAF_DIR).join(&contract_hex);
            let src_leaf_file_path = src_leaf_path.join(ELEMENT_FILE);
            let dst_leaf_file_path = dst_leaf_path.join(ELEMENT_FILE);
            if src_leaf_file_path.is_file() {
                fs::rename(&src_leaf_file_path, dst_leaf_file_path)?;
            }
            fs::remove_dir(src_leaf_path)?;
        }

        fs::remove_file(base_info_path)?;
        let _ = fs::remove_file(tree_pos_path);
        let _ = fs::remove_file(tree_pos_opt_path);
        fs::remove_dir(commit_path)?;

        Ok(())
    }
}
