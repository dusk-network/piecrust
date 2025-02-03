// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::collections::btree_map::Entry::{Occupied, Vacant};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::{io, mem};

use dusk_wasmtime::Engine;
use piecrust_uplink::ContractId;

use crate::contract::ContractMetadata;
use crate::store::tree::{
    position_from_contract, ContractIndexElement, Hash, PageOpening, TreePos,
};
use crate::store::{
    base_from_path, Bytecode, Call, Commit, CommitStore, Memory, Metadata,
    Module, BASE_FILE, BYTECODE_DIR, DEFAULT_MASTER_VERSION, ELEMENT_FILE,
    MAIN_DIR, MEMORY_DIR, METADATA_EXTENSION, OBJECTCODE_EXTENSION, PAGE_SIZE,
};
use crate::storeroom::Storeroom;
use crate::Error;

#[derive(Debug, Clone)]
pub struct ContractDataEntry {
    pub bytecode: Bytecode,
    pub module: Module,
    pub metadata: Metadata,
    pub memory: Memory,
    pub is_new: bool,
}

/// The representation of a session with a [`ContractStore`].
///
/// A session tracks modifications to the contracts' memories by keeping
/// references to the set of instantiated contracts.
///
/// The modifications are kept in memory and are only persisted to disk on a
/// call to [`commit`].
///
/// [`commit`]: ContractSession::commit
pub struct ContractSession {
    contracts: BTreeMap<ContractId, ContractDataEntry>,
    engine: Engine,

    base: Option<Commit>,
    root_dir: PathBuf,

    call: mpsc::Sender<Call>,

    commit_store: Arc<Mutex<CommitStore>>,

    #[allow(dead_code)]
    storeroom: Storeroom,
}

pub type ContractMemTree = dusk_merkle::Tree<Hash, 32, 2>;

impl Debug for ContractSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContractSession")
            .field("contracts", &self.contracts)
            .field("base", &self.base)
            .field("root_dir", &self.root_dir)
            .finish()
    }
}

impl ContractSession {
    pub(crate) fn new<P: AsRef<Path>>(
        root_dir: P,
        engine: Engine,
        base: Option<Commit>,
        call: mpsc::Sender<Call>,
        commit_store: Arc<Mutex<CommitStore>>,
        storeroom: Storeroom,
    ) -> Self {
        Self {
            contracts: BTreeMap::new(),
            engine,
            base,
            root_dir: root_dir.as_ref().into(),
            call,
            commit_store,
            storeroom,
        }
    }

    /// Returns the root that the session would have if one would decide to
    /// commit it.
    ///
    /// Keep in mind that modifications to memories obtained using [`contract`],
    /// may cause the root to be inconsistent. The caller should ensure that no
    /// instance of [`Memory`] obtained via this session is being modified when
    /// calling this function.
    ///
    /// [`contract`]: ContractSession::contract
    pub fn root(&self) -> Hash {
        tracing::trace!("root called commit cloning");
        let mut commit = self
            .base
            .as_ref()
            .cloned()
            // .map(|c| c.fast_clone(&mut self.contracts.keys()))
            // .map(|c| c.clone())
            .unwrap_or(Commit::new(&self.commit_store, None));
        for (contract, entry) in &self.contracts {
            commit.insert(*contract, &entry.memory);
        }
        let root = commit.root();
        tracing::trace!("root call finished");

        *root
    }

    /// Returns an iterator through all the pages of a contract, together with a
    /// proof of their inclusion in the state.
    pub fn memory_pages(
        &self,
        contract: ContractId,
    ) -> Option<impl Iterator<Item = (usize, &[u8], PageOpening)>> {
        tracing::trace!("memory_pages called commit cloning");
        let mut commit = self
            .base
            .clone()
            .unwrap_or(Commit::new(&self.commit_store, None));
        for (contract, entry) in &self.contracts {
            commit.insert(*contract, &entry.memory);
        }

        let contract_data = self.contracts.get(&contract)?;
        let inclusion_proofs = commit.inclusion_proofs(&contract)?;

        let inclusion_proofs =
            inclusion_proofs.map(move |(page_index, opening)| {
                let page_offset = page_index * PAGE_SIZE;
                let page = &contract_data.memory[page_offset..][..PAGE_SIZE];
                (page_index, page, opening)
            });

        Some(inclusion_proofs)
    }

    /// Commits the given session to disk, consuming the session and adding it
    /// to the [`ContractStore`] it was created from.
    ///
    /// Keep in mind that modifications to memories obtained using [`contract`],
    /// may cause the root to be inconsistent. The caller should ensure that no
    /// instance of [`Memory`] obtained via this session is being modified when
    /// calling this function.
    ///
    /// # Safety
    /// This method should only be called once, while immediately allowing the
    /// `ContractSession` to drop.
    ///
    /// [`contract`]: ContractSession::contract
    pub fn commit(&mut self) -> io::Result<Hash> {
        tracing::trace!("commit started");
        let (replier, receiver) = mpsc::sync_channel(1);

        let mut contracts = BTreeMap::new();
        let base = self.base.clone();

        mem::swap(&mut self.contracts, &mut contracts);

        self.call
            .send(Call::Commit {
                contracts,
                base,
                replier,
            })
            .expect("The receiver should never drop before sending");
        tracing::trace!("commit sent");

        receiver
            .recv()
            .expect("The receiver should always receive a reply")
    }

    /// Returns path to a file representing a given commit and page.
    ///
    /// Requires a contract's memory path and a main state path.
    /// Progresses recursively via bases of commits.
    pub fn find_page(
        page_index: usize,
        commit: Option<Hash>,
        memory_path: impl AsRef<Path>,
        main_path: impl AsRef<Path>,
    ) -> Option<PathBuf> {
        match commit {
            None => {
                let path = memory_path.as_ref().join(format!("{page_index}"));
                if path.is_file() {
                    Some(path)
                } else {
                    None
                }
            }
            Some(hash) => {
                let hash_hex = hex::encode(hash.as_bytes());
                let path = memory_path
                    .as_ref()
                    .join(&hash_hex)
                    .join(format!("{page_index}"));
                if path.is_file() {
                    //println!("FIND PAGE RETURNED {:?}", path);
                    Some(path)
                } else {
                    let base_info_path =
                        main_path.as_ref().join(hash_hex).join(BASE_FILE);
                    if let Ok(index) = base_from_path(base_info_path) {
                        Self::find_page(
                            page_index,
                            index.maybe_base,
                            memory_path,
                            main_path,
                        )
                    } else {
                        Self::find_page(
                            page_index,
                            None,
                            memory_path,
                            main_path,
                        )
                    }
                }
            }
        }
    }

    /// Returns path to a file representing a given commit and element.
    ///
    /// Requires a contract's leaf path and a main state path.
    /// Progresses recursively via bases of commits.
    pub fn find_element(
        commit: Option<Hash>,
        leaf_path: impl AsRef<Path>,
        main_path: impl AsRef<Path>,
        depth: u32,
    ) -> Option<(PathBuf, u32)> {
        match commit {
            None => {
                let path = leaf_path.as_ref().join(ELEMENT_FILE);
                if path.is_file() {
                    Some((path, u32::MAX))
                } else {
                    None
                }
            }
            Some(hash) => {
                let hash_hex = hex::encode(hash.as_bytes());
                let path =
                    leaf_path.as_ref().join(&hash_hex).join(ELEMENT_FILE);
                if path.is_file() {
                    Some((path, depth + 1))
                } else {
                    let base_info_path =
                        main_path.as_ref().join(hash_hex).join(BASE_FILE);
                    if let Ok(index) = base_from_path(base_info_path) {
                        Self::find_element(
                            index.maybe_base,
                            leaf_path,
                            main_path,
                            depth + 1,
                        )
                    } else {
                        Self::find_element(
                            None,
                            leaf_path,
                            main_path,
                            depth + 1,
                        )
                    }
                }
            }
        }
    }

    /// Return the bytecode and memory belonging to the given `contract`, if it
    /// exists.
    ///
    /// The contract is considered to exist if either of the following
    /// conditions are met:
    ///
    /// - The contract has been [`deploy`]ed in this session
    /// - The contract was deployed to the base commit
    ///
    /// [`deploy`]: ContractSession::deploy
    pub fn contract(
        &mut self,
        contract: ContractId,
    ) -> io::Result<Option<ContractDataEntry>> {
        let commit_id = self.base.as_ref().map(|commit| *commit.root());
        let storeroom = self.storeroom.clone();
        let version = commit_id
            .as_ref()
            .map(|h| hex::encode(h.as_bytes()))
            .unwrap_or(DEFAULT_MASTER_VERSION.to_string());
        match self.contracts.entry(contract) {
            Vacant(entry) => match &self.base {
                None => Ok(None),
                Some(base_commit) => {
                    match base_commit.index_get(&contract).is_some() {
                        true => {
                            let base_dir = self.root_dir.join(MAIN_DIR);

                            let contract_hex = hex::encode(contract);

                            let bytecode_path =
                                base_dir.join(BYTECODE_DIR).join(&contract_hex);
                            let module_path = bytecode_path
                                .with_extension(OBJECTCODE_EXTENSION);
                            let metadata_path = bytecode_path
                                .with_extension(METADATA_EXTENSION);
                            let memory_path =
                                base_dir.join(MEMORY_DIR).join(&contract_hex);

                            let bytecode = Bytecode::from_file(bytecode_path)?;
                            let module =
                                Module::from_file(&self.engine, module_path)?;
                            let metadata = Metadata::from_file(metadata_path)?;

                            let memory = match base_commit.index_get(&contract)
                            {
                                Some(elem) => {
                                    let page_indices =
                                        elem.page_indices().clone();
                                    Memory::from_files(
                                        module.is_64(),
                                        move |page_index: usize| {
                                            match page_indices
                                                .contains(&page_index)
                                            {
                                                true => {
                                                    let x = Self::find_page(
                                                        page_index,
                                                        commit_id,
                                                        &memory_path,
                                                        &base_dir,
                                                    );
                                                    if x.is_some() {
                                                        x
                                                    } else {
                                                        // memory_path.join(
                                                        //     format!(
                                                        //         "{page_index}"
                                                        //     ),
                                                        // ),
                                                        Some(storeroom.retrieve(
                                                            &version,
                                                            &contract_hex,
                                                            format!("{page_index}")
                                                            // ).ok()?.unwrap_or(PathBuf::new()) // todo: errors are suppressed here
                                                        ).ok()?.expect("mem page should exist"))
                                                    }
                                                }
                                                false => None,
                                            }
                                        },
                                        elem.len(),
                                    )?
                                }
                                None => Memory::new(module.is_64())?,
                            };

                            let contract = entry
                                .insert(ContractDataEntry {
                                    bytecode,
                                    module,
                                    metadata,
                                    memory,
                                    is_new: false,
                                })
                                .clone();

                            Ok(Some(contract))
                        }
                        false => Ok(None),
                    }
                }
            },
            Occupied(entry) => Ok(Some(entry.get().clone())),
        }
    }

    /// Remove the given contract from the session.
    pub fn remove_contract(&mut self, contract: &ContractId) {
        self.contracts.remove(contract);
    }

    /// Checks if contract is deployed
    pub fn contract_deployed(&mut self, contract_id: ContractId) -> bool {
        if self.contracts.contains_key(&contract_id) {
            true
        } else if let Some(base_commit) = &self.base {
            base_commit.index_get(&contract_id).is_some()
        } else {
            false
        }
    }

    /// Deploys bytecode to the contract store with the given its `contract_id`.
    ///
    /// See [`deploy`] for deploying bytecode without specifying a contract ID.
    ///
    /// [`deploy`]: ContractSession::deploy
    pub fn deploy<B: AsRef<[u8]>>(
        &mut self,
        contract_id: ContractId,
        bytecode: B,
        module: B,
        metadata: ContractMetadata,
        metadata_bytes: B,
    ) -> io::Result<()> {
        let bytecode = Bytecode::new(bytecode)?;
        let module = Module::new(&self.engine, module)?;
        let metadata = Metadata::new(metadata_bytes, metadata)?;
        let memory = Memory::new(module.is_64())?;

        // If the position is already filled in the tree, the contract cannot be
        // inserted.
        if let Some(base) = self.base.as_ref() {
            if base.index_get(&contract_id).is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("Existing contract '{contract_id}'"),
                ));
            }
        }

        self.contracts.insert(
            contract_id,
            ContractDataEntry {
                bytecode,
                module,
                metadata,
                memory,
                is_new: true,
            },
        );

        Ok(())
    }

    /// Remove the `old_contract` and move the `new_contract` to the
    /// `old_contract`, effectively replacing the `old_contract` with
    /// `new_contract`.
    pub fn replace(
        &mut self,
        old_contract: ContractId,
        new_contract: ContractId,
    ) -> Result<(), Error> {
        let mut new_contract_data =
            self.contracts.remove(&new_contract).ok_or_else(|| {
                Error::PersistenceError(Arc::new(io::Error::new(
                    io::ErrorKind::Other,
                    format!("Contract '{new_contract}' not found"),
                )))
            })?;

        // The new contract should have the ID of the old contract in its
        // metadata.
        new_contract_data.metadata.set_data(ContractMetadata {
            contract_id: old_contract,
            owner: new_contract_data.metadata.data().owner.clone(),
        })?;

        self.contracts.insert(old_contract, new_contract_data);

        Ok(())
    }

    /// Provides metadata of the contract with a given `contract_id`.
    pub fn contract_metadata(
        &mut self,
        contract_id: &ContractId,
    ) -> Option<&ContractMetadata> {
        let _ = self.contract(*contract_id);
        self.contracts
            .get(contract_id)
            .map(|store_data| store_data.metadata.data())
    }

    pub fn calculate_root(
        entries: impl Iterator<Item = ([u8; 32], u64)>,
    ) -> [u8; 32] {
        let mut tree = ContractMemTree::new();
        for (hash, int_pos) in entries {
            tree.insert(int_pos, hash);
        }
        let r = *(*tree.root()).as_bytes();
        r
    }

    pub fn calculate_root_pos_32(
        entries: impl Iterator<Item = ([u8; 32], u32)>,
    ) -> [u8; 32] {
        let mut tree = ContractMemTree::new();
        for (hash, int_pos) in entries {
            let int_pos = int_pos as u64;
            tree.insert(int_pos, hash);
        }
        let r = *(*tree.root()).as_bytes();
        r
    }

    pub fn contract_prefix(contract: &ContractId) -> String {
        let mut a = [0u8; 8];
        a.copy_from_slice(&contract.to_bytes()[..8]);
        hex::encode(a)
    }

    pub fn print_root_infos(
        elements: &BTreeMap<ContractId, ContractIndexElement>,
        new_elements: &BTreeMap<ContractId, ContractIndexElement>,
        tree_pos: &TreePos,
    ) -> Result<(), Error> {
        // println!();
        // println!("tree_pos");
        let mut tree_pos_map: HashMap<u64, [u8; 32]> = HashMap::new();
        for (k, (h, _c)) in tree_pos.iter() {
            // println!(
            //     "{} {} {}",
            //     *k,
            //     hex::encode(h),
            //     hex::encode((*c).to_le_bytes())
            // );
            tree_pos_map.insert(*k as u64, *h.as_bytes());
        }

        println!();
        println!("elems:");
        let mut sorted_elements: Vec<(ContractId, ContractIndexElement)> =
            elements.iter().map(|(a, b)| (*a, b.clone())).collect();
        sorted_elements.sort_by(|(_, el1), (_, el2)| {
            el1.int_pos()
                .expect("int_pos")
                .cmp(&el2.int_pos().expect("int_pos"))
        });
        for (contract_id, element) in sorted_elements.iter() {
            let is_new_element = new_elements.contains_key(contract_id);
            let contract_pos_hex =
                hex::encode(position_from_contract(contract_id).to_le_bytes());
            if Some(element.hash().expect("hash should exist").as_bytes())
                != tree_pos_map
                    .get(&element.int_pos().expect("int_pos should exist"))
            {
                print!("* ");
                print!(
                    "{} {} ({}) int_pos={} from tree_pos={} is_new={}",
                    hex::encode(element.hash().expect("hash should exist")),
                    Self::contract_prefix(contract_id),
                    contract_pos_hex,
                    element.int_pos().expect("int_pos should exist"),
                    hex::encode(
                        tree_pos_map
                            .get(
                                &element
                                    .int_pos()
                                    .expect("int_pos should exist")
                            )
                            .expect("should be found")
                    ),
                    is_new_element,
                );
                println!();
            }
        }

        let root_from_elements =
            Self::calculate_root(elements.iter().map(|(_, el)| {
                (
                    *el.hash().expect("hash should exist").as_bytes(),
                    el.int_pos().expect("int_pos should exist"),
                )
            }));
        println!();
        println!("root_from_elements={}", hex::encode(root_from_elements));
        let root_from_tree_pos_file = Self::calculate_root_pos_32(
            tree_pos.iter().map(|(k, (h, _c))| (*h.as_bytes(), *k)),
        );
        println!();
        println!(
            "root_from_tree_pos_file={}",
            hex::encode(root_from_tree_pos_file)
        );

        println!();
        Ok(())
    }
}

impl Drop for ContractSession {
    fn drop(&mut self) {
        if let Some(base) = self.base.take() {
            let root = base.root();
            let _ = self.call.send(Call::SessionDrop(*root));
        }
    }
}
