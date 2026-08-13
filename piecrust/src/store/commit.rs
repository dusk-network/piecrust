// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

pub mod finalizer;
pub mod operation;
pub mod reader;
pub mod remover;
pub mod writer;

use std::cell::Ref;
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::{fs, io};

use piecrust_uplink::ContractId;
use tracing::debug;

use crate::PageOpening;
use crate::store::baseinfo::BaseInfo;
use crate::store::commit_store::CommitStore;
use crate::store::hasher::Hash;
use crate::store::index::{ContractIndexElement, NewContractIndex};
use crate::store::tree::{ContractsMerkle, position_from_contract};
use crate::store::{ContractSession, Memory};

#[derive(Debug, Clone)]
pub(crate) struct Commit {
    index: NewContractIndex,
    contracts_merkle: ContractsMerkle,
    maybe_hash: Option<Hash>,
    commit_store: Option<Arc<Mutex<CommitStore>>>,
    base: Option<Hash>,
}

impl Commit {
    pub fn new(
        commit_store: &Arc<Mutex<CommitStore>>,
        maybe_base: Option<Hash>,
    ) -> Self {
        Self {
            index: NewContractIndex::new(),
            contracts_merkle: ContractsMerkle::default(),
            maybe_hash: None,
            commit_store: Some(commit_store.clone()),
            base: maybe_base,
        }
    }

    pub fn fast_clone<'a>(
        &self,
        contract_ids: impl Iterator<Item = &'a ContractId>,
    ) -> Self {
        let mut index = NewContractIndex::new();
        for contract_id in contract_ids {
            if let Some(a) = self.index.get(contract_id) {
                index.insert_contract_index(contract_id, a.clone());
            }
        }
        Self {
            index,
            contracts_merkle: self.contracts_merkle.clone(),
            maybe_hash: self.maybe_hash,
            commit_store: self.commit_store.clone(),
            base: self.base,
        }
    }

    #[allow(dead_code)]
    pub fn inclusion_proofs(
        mut self,
        contract_id: &ContractId,
    ) -> Option<impl Iterator<Item = (usize, PageOpening)> + use<>> {
        let contract = self.index.remove_contract_index(contract_id)?;

        let pos = position_from_contract(contract_id);

        let (iter, tree) = contract.page_indices_and_tree();
        Some(iter.map(move |page_index| {
            let tree_opening = self
                .contracts_merkle
                .opening(pos)
                .expect("There must be a leaf for the contract");

            let page_opening = tree
                .opening(page_index as u64)
                .expect("There must be a leaf for the page");

            (
                page_index,
                PageOpening {
                    tree: tree_opening,
                    inner: page_opening,
                },
            )
        }))
    }

    pub fn insert_contract(
        &mut self,
        contract_id: ContractId,
        memory: &Memory,
        is_new: bool,
    ) -> BTreeSet<usize> {
        if is_new {
            self.remove_and_insert(contract_id, memory)
        } else {
            self.insert(contract_id, memory)
        }
    }

    fn insert(
        &mut self,
        contract_id: ContractId,
        memory: &Memory,
    ) -> BTreeSet<usize> {
        self.insert_inner(contract_id, memory, true)
    }

    fn insert_inner(
        &mut self,
        contract_id: ContractId,
        memory: &Memory,
        allow_identical_skip: bool,
    ) -> BTreeSet<usize> {
        if self.index.get(&contract_id).is_none() {
            if let Some(element) = self
                .commit_store
                .as_ref()
                .expect("commit store should exist")
                .lock()
                .unwrap()
                .get_from_main_index(&contract_id)
            {
                self.index
                    .insert_contract_index(&contract_id, element.clone())
            } else {
                self.index.insert_contract_index(
                    &contract_id,
                    ContractIndexElement::new(memory.is_64()),
                );
            }
        }
        let (element, contracts_merkle) =
            self.element_and_merkle_mut(&contract_id);
        let element = element.unwrap();

        element.set_len(memory.current_len());

        debug!("Check dirty pages for {contract_id}");
        let mut identical_pages = BTreeSet::new();
        for (dirty_page, clean_page, page_index) in memory.dirty_pages() {
            if allow_identical_skip
                && element.page_indices().contains(page_index)
                && dirty_page == clean_page
            {
                debug!(
                    msg = "skip identical page",
                    page_index,
                    contract_id = hex::encode(&contract_id.as_bytes()[0..8]),
                );
                identical_pages.insert(*page_index);
                continue;
            }

            let hash = Hash::new(dirty_page);
            debug!(
                msg = "insert page",
                page_index,
                contract_id = hex::encode(&contract_id.as_bytes()[0..8]),
                dirty = hex::encode(hash.as_bytes()),
            );

            element.insert_page_index_hash(
                *page_index,
                *page_index as u64,
                hash,
            );
        }

        let root = *element.tree().root();
        let pos = position_from_contract(&contract_id);
        let internal_pos = contracts_merkle.insert(pos, root);
        element.set_hash(Some(root));
        element.set_int_pos(Some(internal_pos));

        identical_pages
    }

    fn remove_and_insert(
        &mut self,
        contract: ContractId,
        memory: &Memory,
    ) -> BTreeSet<usize> {
        self.index.remove_contract_index(&contract);
        self.index.insert_contract_index(
            &contract,
            ContractIndexElement::new(memory.is_64()),
        );
        self.insert_inner(contract, memory, false)
    }

    fn redundant_elements(&self) -> Vec<ContractId> {
        let mut to_remove = vec![];
        for (c, e) in self.index().iter() {
            if let Some(h) = e.hash {
                let mut commit_store = self
                    .commit_store
                    .as_ref()
                    .expect("commit store present")
                    .lock()
                    .unwrap();
                if let Some(mel) = commit_store.get_from_main_index(c) {
                    if mel.hash() == Some(h) {
                        to_remove.push(*c)
                    }
                }
            }
        }
        to_remove
    }

    /// remove commit-specific elements if they are the same
    /// as the corresponding elements in main
    pub fn squash(&mut self) {
        let to_remove = self.redundant_elements();
        for c in to_remove.iter() {
            self.index_mut().remove_contract_index(c);
        }
    }

    pub fn root(&self) -> Ref<Hash> {
        tracing::trace!("calculating root started");
        let ret = self.contracts_merkle.root();
        tracing::trace!("calculating root finished");
        ret
    }

    pub fn index_get(
        &self,
        contract_id: &ContractId,
    ) -> Option<&ContractIndexElement> {
        Hulk::deep_index_get(
            &self.index,
            *contract_id,
            self.commit_store.clone(),
            self.base,
        )
        .map(|a| unsafe { &*a })
    }

    pub fn index(&self) -> &NewContractIndex {
        &self.index
    }

    pub fn index_mut(&mut self) -> &mut NewContractIndex {
        &mut self.index
    }

    pub fn base(&self) -> Option<Hash> {
        self.base
    }

    pub fn validate_persisted_state(
        &self,
        root_dir: &Path,
        root: Hash,
    ) -> io::Result<()> {
        let main_dir = root_dir.join(crate::store::MAIN_DIR);
        let base_info = BaseInfo::from_path(
            main_dir
                .join(hex::encode(root))
                .join(crate::store::BASE_FILE),
        )?;

        for contract in &base_info.contract_hints {
            let expected = self.index_get(contract).ok_or_else(|| {
                invalid_state(contract, "missing index entry")
            })?;
            let contract_hex = hex::encode(contract);
            let leaf_path =
                main_dir.join(crate::store::LEAF_DIR).join(&contract_hex);
            let (element_path, _) = ContractSession::find_element(
                Some(root),
                &leaf_path,
                &main_dir,
                0,
            )
            .ok_or_else(|| invalid_state(contract, "missing index element"))?;
            let bytes = fs::read(element_path)?;
            let actual: ContractIndexElement = rkyv::from_bytes(&bytes)
                .map_err(|error| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        error.to_string(),
                    )
                })?;

            if actual.len() != expected.len()
                || actual.page_indices() != expected.page_indices()
                || actual.hash() != expected.hash()
                || actual.int_pos() != expected.int_pos()
                || *actual.tree().root() != *expected.tree().root()
            {
                return Err(invalid_state(
                    contract,
                    "mismatched index element",
                ));
            }

            let tree_opening = self
                .contracts_merkle
                .opening(position_from_contract(contract))
                .ok_or_else(|| {
                    invalid_state(contract, "missing tree opening")
                })?;
            if !tree_opening.verify(*expected.tree().root()) {
                return Err(invalid_state(contract, "invalid tree opening"));
            }

            let memory_path =
                main_dir.join(crate::store::MEMORY_DIR).join(&contract_hex);
            let commit_memory_path = memory_path.join(hex::encode(root));
            match fs::symlink_metadata(&commit_memory_path) {
                Ok(metadata) if !metadata.is_dir() => {
                    return Err(invalid_state(
                        contract,
                        "invalid memory directory",
                    ));
                }
                Ok(_) => {}
                Err(error)
                    if error.kind() == io::ErrorKind::NotFound
                        && expected.page_indices().is_empty() => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    return Err(invalid_state(
                        contract,
                        "missing memory directory",
                    ));
                }
                Err(error) => return Err(error),
            }
            for page_index in expected.page_indices() {
                let page_path = ContractSession::find_page(
                    *page_index,
                    Some(root),
                    &memory_path,
                    &main_dir,
                )
                .ok_or_else(|| {
                    invalid_state(contract, "missing memory page")
                })?;
                let page = fs::read(page_path)?;
                let inner =
                    expected.tree().opening(*page_index as u64).ok_or_else(
                        || invalid_state(contract, "missing page opening"),
                    )?;
                let opening = PageOpening {
                    tree: tree_opening,
                    inner,
                };
                if !opening.verify(&page) {
                    return Err(invalid_state(
                        contract,
                        "mismatched memory page",
                    ));
                }
            }
        }

        Ok(())
    }

    pub fn element_and_merkle_mut(
        &mut self,
        contract_id: &ContractId,
    ) -> (Option<&mut ContractIndexElement>, &mut ContractsMerkle) {
        (
            Hulk::deep_index_get_mut(
                &mut self.index,
                *contract_id,
                self.commit_store.clone(),
                self.base,
            )
            .map(|a| unsafe { &mut *a }),
            &mut self.contracts_merkle,
        )
    }
}

fn invalid_state(contract: &ContractId, detail: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!(
            "Invalid persisted state for contract {}: {detail}",
            hex::encode(contract)
        ),
    )
}

#[derive(Debug, Clone)]
pub(crate) struct Hulk;

impl Hulk {
    pub fn deep_index_get(
        index: &NewContractIndex,
        contract_id: ContractId,
        commit_store: Option<Arc<Mutex<CommitStore>>>,
        base: Option<Hash>,
    ) -> Option<*const ContractIndexElement> {
        if let Some(e) = index.get(&contract_id) {
            return Some(e);
        }
        let mut base = base?;
        let commit_store = commit_store.clone()?;
        let commit_store = commit_store.lock().unwrap();
        loop {
            let (maybe_element, commit_base) =
                commit_store.get_element_and_base(&base, &contract_id);
            if let Some(e) = maybe_element {
                return Some(e);
            }
            base = commit_base?;
        }
    }

    pub fn deep_index_get_mut(
        index: &mut NewContractIndex,
        contract_id: ContractId,
        commit_store: Option<Arc<Mutex<CommitStore>>>,
        base: Option<Hash>,
    ) -> Option<*mut ContractIndexElement> {
        if let Some(e) = index.get_mut(&contract_id) {
            return Some(e);
        }
        let mut base = base?;
        let commit_store = commit_store.clone()?;
        let mut commit_store = commit_store.lock().unwrap();
        loop {
            let (maybe_element, commit_base) =
                commit_store.get_element_and_base_mut(&base, &contract_id);
            if let Some(e) = maybe_element {
                return Some(e);
            }
            base = commit_base?;
        }
    }
}
