// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::cell::Ref;
use std::sync::{Arc, Mutex};

use crate::store::commit_hulk::CommitHulk;
use crate::store::commit_store::CommitStore;
use crate::store::hasher::Hash;
use crate::store::index::{ContractIndexElement, NewContractIndex};
use crate::store::tree::{position_from_contract, ContractsMerkle};
use crate::store::Memory;
use crate::PageOpening;
use piecrust_uplink::ContractId;

#[derive(Debug, Clone)]
pub(crate) struct Commit {
    pub index: NewContractIndex,
    pub contracts_merkle: ContractsMerkle,
    pub maybe_hash: Option<Hash>,
    pub commit_store: Option<Arc<Mutex<CommitStore>>>,
    pub base: Option<Hash>,
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

    #[allow(dead_code)]
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

    pub fn to_hulk(&self) -> CommitHulk {
        CommitHulk::from_commit(self)
    }

    #[allow(dead_code)]
    pub fn inclusion_proofs(
        mut self,
        contract_id: &ContractId,
    ) -> Option<impl Iterator<Item = (usize, PageOpening)>> {
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

    pub fn insert(&mut self, contract_id: ContractId, memory: &Memory) {
        if self.index_get(&contract_id).is_none() {
            self.index.insert_contract_index(
                &contract_id,
                ContractIndexElement::new(memory.is_64()),
            );
        }
        let (element, contracts_merkle) =
            self.element_and_merkle_mut(&contract_id);
        let element = element.unwrap();

        element.set_len(memory.current_len);

        for (dirty_page, _, page_index) in memory.dirty_pages() {
            let hash = Hash::new(dirty_page);
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
    }

    pub fn remove_and_insert(&mut self, contract: ContractId, memory: &Memory) {
        self.index.remove_contract_index(&contract);
        self.insert(contract, memory);
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
        self.index.get(contract_id)
    }

    pub fn element_and_merkle_mut(
        &mut self,
        contract_id: &ContractId,
    ) -> (Option<&mut ContractIndexElement>, &mut ContractsMerkle) {
        (self.index.get_mut(contract_id), &mut self.contracts_merkle)
    }
}
