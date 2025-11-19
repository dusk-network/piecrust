// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use crate::store::baseinfo::BaseInfo;
use crate::store::commit::Commit;
use crate::store::commit_store::CommitStore;
use crate::store::hasher::Hash;
use crate::store::index::{ContractIndexElement, NewContractIndex};
use crate::store::tree::{position_from_contract, ContractsMerkle};
use crate::store::treepos::TreePos;
use crate::store::{
    Bytecode, ContractSession, Module, BASE_FILE, BYTECODE_DIR, LEAF_DIR,
    MAIN_DIR, MEMORY_DIR, OBJECTCODE_EXTENSION, TREE_POS_FILE,
    TREE_POS_OPT_FILE,
};
use dusk_wasmtime::Engine;
use piecrust_uplink::ContractId;
use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::BufReader;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::{fs, io};
use tracing::info;

pub struct CommitReader;

impl CommitReader {
    ///
    /// Reads all commits into a given commit store
    pub fn read_all_commits<P: AsRef<Path>>(
        engine: &Engine,
        root_dir: P,
        commit_store: Arc<Mutex<CommitStore>>,
    ) -> io::Result<()> {
        let root_dir = root_dir.as_ref();

        let root_dir = root_dir.join(MAIN_DIR);
        fs::create_dir_all(&root_dir)?;

        for entry in fs::read_dir(root_dir)? {
            let entry = entry?;
            if entry.path().is_dir() {
                let filename = entry.file_name();
                if filename == MEMORY_DIR
                    || filename == BYTECODE_DIR
                    || filename == LEAF_DIR
                {
                    continue;
                }
                tracing::trace!("before read_commit");
                let commit = Self::commit_from_dir(
                    engine,
                    entry.path(),
                    commit_store.clone(),
                )?;
                tracing::trace!("after read_commit");
                let root = *commit.root();
                commit_store.lock().unwrap().insert_commit(root, commit);
            }
        }

        Ok(())
    }

    fn commit_from_dir<P: AsRef<Path>>(
        engine: &Engine,
        dir: P,
        commit_store: Arc<Mutex<CommitStore>>,
    ) -> io::Result<Commit> {
        let dir = dir.as_ref();
        let mut commit_id: Option<String> = None;
        let main_dir = if dir
            .file_name()
            .expect("Filename or folder name should exist")
            != MAIN_DIR
        {
            commit_id = Some(
                dir.file_name()
                    .expect("Filename or folder name should exist")
                    .to_string_lossy()
                    .to_string(),
            );
            // this means we are in a commit dir, need to back up for bytecode
            // and memory paths to work correctly
            dir.parent().expect("Parent should exist")
        } else {
            dir
        };
        let maybe_hash = commit_id.as_ref().map(Self::commit_id_to_hash);

        // let contracts_merkle_path = dir.join(MERKLE_FILE);
        let leaf_dir = main_dir.join(LEAF_DIR);
        tracing::trace!("before index_merkle_from_path");

        let tree_pos = if let Some(ref hash_hex) = commit_id {
            let tree_pos_path = main_dir.join(hash_hex).join(TREE_POS_FILE);
            let tree_pos_opt_path =
                main_dir.join(hash_hex).join(TREE_POS_OPT_FILE);
            Self::tree_pos_from_path(tree_pos_path, tree_pos_opt_path)?
        } else {
            None
        };

        let (index, contracts_merkle) = Self::index_merkle_from_path(
            main_dir,
            leaf_dir,
            &maybe_hash,
            commit_store.clone(),
            tree_pos.as_ref(),
            engine,
        )?;
        tracing::trace!("after index_merkle_from_path");

        let bytecode_dir = main_dir.join(BYTECODE_DIR);
        let memory_dir = main_dir.join(MEMORY_DIR);

        for (contract, contract_index) in index.iter() {
            let contract_hex = hex::encode(contract);

            // Check that all contracts in the index file have a corresponding
            // bytecode and memory pages specified.
            let bytecode_path = bytecode_dir.join(&contract_hex);
            if !bytecode_path.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "Non-existing bytecode for contract: {contract_hex}"
                    ),
                ));
            }

            let module_path =
                bytecode_path.with_extension(OBJECTCODE_EXTENSION);

            // SAFETY it is safe to deserialize the file here, since we don't
            // use the module here. We just want to check if the
            // file is valid.
            if Module::from_file(engine, &module_path).is_err() {
                let bytecode = Bytecode::from_file(bytecode_path)?;
                let module = Module::from_bytecode(engine, bytecode.as_ref())
                    .map_err(|err| {
                    io::Error::new(io::ErrorKind::InvalidData, err)
                })?;
                fs::write(module_path, module.serialize())?;
            }

            let contract_memory_dir = memory_dir.join(&contract_hex);

            for page_index in contract_index.page_indices() {
                let page_path = ContractSession::find_page(
                    *page_index,
                    maybe_hash,
                    &contract_memory_dir,
                    main_dir,
                );
                let found = page_path.map(|p| p.is_file()).unwrap_or(false);
                if !found {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "Non-existing memory for contract: {contract_hex}"
                        ),
                    ));
                }
            }
        }

        let base = if let Some(ref hash_hex) = commit_id {
            let base_info_path = main_dir.join(hash_hex).join(BASE_FILE);
            BaseInfo::from_path(base_info_path)?.maybe_base
        } else {
            None
        };

        Ok(Commit {
            index,
            contracts_merkle,
            maybe_hash,
            commit_store: Some(commit_store),
            base,
        })
    }

    fn index_merkle_from_path(
        main_path: impl AsRef<Path>,
        leaf_dir: impl AsRef<Path>,
        maybe_commit_id: &Option<Hash>,
        commit_store: Arc<Mutex<CommitStore>>,
        maybe_tree_pos: Option<&TreePos>,
        engine: &Engine,
    ) -> io::Result<(NewContractIndex, ContractsMerkle)> {
        let leaf_dir = leaf_dir.as_ref();

        let mut index: NewContractIndex = NewContractIndex::new();
        let mut merkle: ContractsMerkle = ContractsMerkle::default();

        let mut merkle_from_elements: BTreeMap<u32, (Hash, u64, ContractId)> =
            BTreeMap::new();

        for entry in fs::read_dir(leaf_dir)? {
            let entry = entry?;
            if entry.path().is_dir() {
                let contract_id_hex =
                    entry.file_name().to_string_lossy().to_string();
                let contract_id = Self::contract_id_from_hex(&contract_id_hex);
                let contract_leaf_path = leaf_dir.join(&contract_id_hex);
                let path_depth_pair = ContractSession::find_element(
                    *maybe_commit_id,
                    &contract_leaf_path,
                    &main_path,
                    0,
                );
                if let Some((element_path, element_depth)) = path_depth_pair {
                    if element_path.is_file() {
                        let element_bytes = fs::read(&element_path)?;
                        let element: ContractIndexElement =
                            rkyv::from_bytes(&element_bytes).map_err(|err| {
                                tracing::trace!(
                                "deserializing element file failed {}",
                                err
                            );
                                io::Error::new(
                                    io::ErrorKind::InvalidData,
                                    format!(
                                        "Invalid element file \"{element_path:?}\": {err}"
                                    ),
                                )
                            })?;
                        if let Some(h) = element.hash() {
                            merkle_from_elements.insert(
                                element.int_pos().expect("internal pos exists")
                                    as u32,
                                (
                                    h,
                                    position_from_contract(&contract_id),
                                    contract_id,
                                ),
                            );
                        }

                        let bytecode_dir =
                            main_path.as_ref().join(BYTECODE_DIR);

                        // Check that all contracts in the index file have a
                        // corresponding bytecode and
                        // memory pages specified.
                        let bytecode_path = bytecode_dir.join(&contract_id_hex);
                        if !bytecode_path.is_file() {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                format!("Non-existing bytecode for contract: {contract_id_hex}"),
                            ));
                        }

                        let module_path =
                            bytecode_path.with_extension(OBJECTCODE_EXTENSION);

                        // SAFETY it is safe to deserialize the file here, since
                        // we don't use the module here.
                        // We just want to check if the
                        // file is valid.
                        if Module::from_file(engine, &module_path).is_err() {
                            let bytecode = Bytecode::from_file(bytecode_path)?;
                            let module = Module::from_bytecode(
                                engine,
                                bytecode.as_ref(),
                            )
                            .map_err(|err| {
                                io::Error::new(io::ErrorKind::InvalidData, err)
                            })?;
                            fs::write(module_path, module.serialize())?;
                            info!("module {contract_id_hex} recompiled");
                        } else {
                            info!("module {contract_id_hex} loaded");
                        }

                        if element_depth != u32::MAX {
                            index.insert_contract_index(&contract_id, element);
                        } else {
                            commit_store
                                .lock()
                                .unwrap()
                                .insert_main_index(&contract_id, element);
                        }
                    }
                }
            }
        }

        match maybe_tree_pos {
            // for backwards compatibility we use TreePos if it exists
            Some(tree_pos) => {
                for (int_pos, (hash, pos)) in tree_pos.iter() {
                    merkle.insert_with_int_pos(*pos, *int_pos as u64, *hash);
                }
            }
            None => {
                // reading Merkle from elements
                for (int_pos, (hash, pos, _)) in merkle_from_elements.iter() {
                    merkle.insert_with_int_pos(*pos, *int_pos as u64, *hash);
                }
            }
        }

        Ok((index, merkle))
    }

    fn contract_id_from_hex<S: AsRef<str>>(contract_id: S) -> ContractId {
        let bytes: [u8; 32] = hex::decode(contract_id.as_ref())
            .expect("Hex decoding of contract id string should succeed")
            .try_into()
            .expect("Contract id string conversion should succeed");
        ContractId::from_bytes(bytes)
    }

    fn commit_id_to_hash<S: AsRef<str>>(commit_id: S) -> Hash {
        let hash: [u8; 32] = hex::decode(commit_id.as_ref())
            .expect("Hex decoding of commit id string should succeed")
            .try_into()
            .expect("Commit id string conversion should succeed");
        Hash::from(hash)
    }

    fn tree_pos_from_path(
        path: impl AsRef<Path>,
        opt_path: impl AsRef<Path>,
    ) -> io::Result<Option<TreePos>> {
        let path = path.as_ref();

        Ok(if opt_path.as_ref().exists() {
            let f = OpenOptions::new().read(true).open(opt_path.as_ref())?;
            let mut buf_f = BufReader::new(f);
            Some(TreePos::unmarshall(&mut buf_f)?)
        } else if path.exists() {
            let tree_pos_bytes = fs::read(path)?;
            Some(rkyv::from_bytes(&tree_pos_bytes).map_err(|err| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Invalid tree positions file \"{path:?}\": {err}"),
                )
            })?)
        } else {
            None
        })
    }
}
