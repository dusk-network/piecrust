// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use crate::store::baseinfo::BaseInfo;
use crate::store::commit::Commit;
use crate::store::commit_store::CommitStore;
use crate::store::hasher::Hash;
use crate::store::session::ContractDataEntry;
use crate::store::{
    BASE_FILE, BYTECODE_DIR, ELEMENT_FILE, LEAF_DIR, MAIN_DIR, MEMORY_DIR,
    METADATA_EXTENSION, OBJECTCODE_EXTENSION,
};
use piecrust_uplink::ContractId;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::{fs, io};

pub struct CommitWriter;

impl CommitWriter {
    ///
    /// Creates and writes commit, adds the created commit to commit store.
    /// The created commit is immutable and its hash (root) is calculated and
    /// returned by this method.
    pub fn create_and_write<P: AsRef<Path>>(
        root_dir: P,
        commit_store: Arc<Mutex<CommitStore>>,
        base: Option<Commit>,
        commit_contracts: BTreeMap<ContractId, ContractDataEntry>,
    ) -> io::Result<Hash> {
        let root_dir = root_dir.as_ref();

        let base_info = BaseInfo {
            maybe_base: base.as_ref().map(|base| *base.root()),
            ..Default::default()
        };

        let mut commit =
            base.unwrap_or(Commit::new(&commit_store, base_info.maybe_base));

        for (contract_id, contract_data) in &commit_contracts {
            if contract_data.is_new {
                commit.remove_and_insert(*contract_id, &contract_data.memory)
            } else {
                commit.insert(*contract_id, &contract_data.memory)
            };
        }

        commit.squash();

        let root = *commit.root();
        let root_hex = hex::encode(root);
        commit.maybe_hash = Some(root);
        commit.base = base_info.maybe_base;

        // Don't write the commit if it already exists on disk. This may happen
        // if the same transactions on the same base commit for example.
        if commit_store.lock().unwrap().contains_key(&root) {
            return Ok(root);
        }

        Self::write_commit_inner(
            root_dir,
            &commit,
            commit_contracts,
            root_hex,
            base_info,
        )
        .map(|_| {
            commit_store.lock().unwrap().insert_commit(root, commit);
            root
        })
    }

    /// Writes a commit to disk.
    fn write_commit_inner<P: AsRef<Path>, S: AsRef<str>>(
        root_dir: P,
        commit: &Commit,
        commit_contracts: BTreeMap<ContractId, ContractDataEntry>,
        commit_id: S,
        mut base_info: BaseInfo,
    ) -> io::Result<()> {
        let root_dir = root_dir.as_ref();

        struct Directories {
            main_dir: PathBuf,
            bytecode_main_dir: PathBuf,
            memory_main_dir: PathBuf,
            leaf_main_dir: PathBuf,
        }

        let directories = {
            let main_dir = root_dir.join(MAIN_DIR);
            fs::create_dir_all(&main_dir)?;

            let bytecode_main_dir = main_dir.join(BYTECODE_DIR);
            fs::create_dir_all(&bytecode_main_dir)?;

            let memory_main_dir = main_dir.join(MEMORY_DIR);
            fs::create_dir_all(&memory_main_dir)?;

            let leaf_main_dir = main_dir.join(LEAF_DIR);
            fs::create_dir_all(&leaf_main_dir)?;

            Directories {
                main_dir,
                bytecode_main_dir,
                memory_main_dir,
                leaf_main_dir,
            }
        };

        // Write the dirty pages contracts of contracts to disk.
        for (contract, contract_data) in &commit_contracts {
            let contract_hex = hex::encode(contract);

            let memory_main_dir =
                directories.memory_main_dir.join(&contract_hex);
            fs::create_dir_all(&memory_main_dir)?;

            let leaf_main_dir = directories.leaf_main_dir.join(&contract_hex);
            fs::create_dir_all(&leaf_main_dir)?;

            let mut pages = BTreeSet::new();

            let mut dirty = false;
            // Write dirty pages and keep track of the page indices.
            if let Some(dirty_pages) = contract_data.memory.dirty_pages() {
                for (dirty_page, _, page_index) in dirty_pages {
                    let page_path: PathBuf = Self::page_path_main(
                        &memory_main_dir,
                        *page_index,
                        &commit_id,
                    )?;
                    fs::write(page_path, dirty_page)?;
                    pages.insert(*page_index);
                    dirty = true;
                }
            }

            let bytecode_main_path =
                directories.bytecode_main_dir.join(&contract_hex);
            let module_main_path =
                bytecode_main_path.with_extension(OBJECTCODE_EXTENSION);
            let metadata_main_path =
                bytecode_main_path.with_extension(METADATA_EXTENSION);

            // If the contract is new, we write the bytecode, module, and
            // metadata files to disk.
            if contract_data.is_new {
                // we write them to the main location
                fs::write(bytecode_main_path, &contract_data.bytecode)?;
                fs::write(module_main_path, contract_data.module.serialize())?;
                fs::write(metadata_main_path, &contract_data.metadata)?;
                dirty = true;
            }
            if dirty {
                base_info.contract_hints.push(*contract);
            }
        }

        tracing::trace!("persisting index started");
        for (contract_id, element) in commit.index.iter() {
            if commit_contracts.contains_key(contract_id) {
                let element_dir_path = directories
                    .leaf_main_dir
                    .join(hex::encode(contract_id.as_bytes()))
                    .join(commit_id.as_ref());
                let element_file_path = element_dir_path.join(ELEMENT_FILE);
                fs::create_dir_all(element_dir_path)?;
                let element_bytes =
                    rkyv::to_bytes::<_, 128>(element).map_err(|err| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("Failed serializing element file: {err}"),
                        )
                    })?;
                fs::write(&element_file_path, element_bytes)?;
            }
        }
        tracing::trace!("persisting index finished");

        let base_main_path =
            Self::base_path_main(&directories.main_dir, commit_id.as_ref())?;
        let base_info_bytes =
            rkyv::to_bytes::<_, 128>(&base_info).map_err(|err| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Failed serializing base info file: {err}"),
                )
            })?;
        fs::write(base_main_path, base_info_bytes)?;

        Ok(())
    }

    fn page_path_main<P: AsRef<Path>, S: AsRef<str>>(
        memory_dir: P,
        page_index: usize,
        commit_id: S,
    ) -> io::Result<PathBuf> {
        let commit_id = commit_id.as_ref();
        let dir = memory_dir.as_ref().join(commit_id);
        fs::create_dir_all(&dir)?;
        Ok(dir.join(format!("{page_index}")))
    }

    fn base_path_main<P: AsRef<Path>, S: AsRef<str>>(
        main_dir: P,
        commit_id: S,
    ) -> io::Result<PathBuf> {
        let commit_id = commit_id.as_ref();
        let dir = main_dir.as_ref().join(commit_id);
        fs::create_dir_all(&dir)?;
        Ok(dir.join(BASE_FILE))
    }
}
