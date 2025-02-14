// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::cell::Ref;
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use crate::store::commit::Commit;
use crate::store::commit_store::CommitStore;
use crate::store::hasher::Hash;
use crate::store::tree::{
    position_from_contract, ContractIndexElement, ContractsMerkle,
    NewContractIndex, PageTree,
};
use crate::store::Memory;
use crate::PageOpening;
use piecrust_uplink::ContractId;

#[derive(Debug, Clone)]
pub(crate) struct CommitHulk {
    index: Option<*const NewContractIndex>,
    index2: NewContractIndex,
    contracts_merkle: ContractsMerkle,
    maybe_hash: Option<Hash>,
    commit_store: Option<Arc<Mutex<CommitStore>>>,
}

impl CommitHulk {
    pub fn from_commit(commit: &Commit) -> Self {
        Self {
            index: Some(&commit.index),
            index2: NewContractIndex::new(),
            contracts_merkle: commit.contracts_merkle.clone(),
            maybe_hash: commit.maybe_hash,
            commit_store: commit.commit_store.clone(),
        }
    }

    pub fn new() -> Self {
        Self {
            index: None,
            index2: NewContractIndex::new(),
            contracts_merkle: ContractsMerkle::default(),
            maybe_hash: None,
            commit_store: None,
        }
    }

    pub fn to_commit(&self) -> Commit {
        let index = self.index.map(|p| unsafe { p.as_ref().unwrap() });
        match index {
            Some(p) => Commit {
                index: p.clone(),
                contracts_merkle: self.contracts_merkle.clone(),
                maybe_hash: self.maybe_hash,
                commit_store: self.commit_store.clone(),
                base: None,
            },
            None => Commit {
                index: NewContractIndex::new(),
                contracts_merkle: self.contracts_merkle.clone(),
                maybe_hash: self.maybe_hash,
                commit_store: self.commit_store.clone(),
                base: None,
            },
        }
    }

    pub fn fast_clone<'a>(
        &self,
        contract_ids: impl Iterator<Item = &'a ContractId>,
    ) -> Self {
        let mut index2 = NewContractIndex::new();
        for contract_id in contract_ids {
            if let Some(a) = self.index_get(contract_id) {
                index2.insert_contract_index(contract_id, a.clone());
            }
        }
        Self {
            index: None,
            index2,
            contracts_merkle: self.contracts_merkle.clone(),
            maybe_hash: self.maybe_hash,
            commit_store: self.commit_store.clone(),
        }
    }

    pub fn inclusion_proofs(
        mut self,
        contract_id: &ContractId,
    ) -> Option<impl Iterator<Item = (usize, PageOpening)>> {
        let contract = self.remove_contract_index(contract_id)?;

        let pos = position_from_contract(contract_id);

        Some(contract.page_indices.into_iter().map(move |page_index| {
            let tree_opening = self
                .contracts_merkle
                .opening(pos)
                .expect("There must be a leaf for the contract");

            let page_opening = contract
                .tree
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
            self.insert_contract_index(
                &contract_id,
                ContractIndexElement {
                    tree: PageTree::new(memory.is_64()),
                    len: 0,
                    page_indices: BTreeSet::new(),
                    hash: None,
                    int_pos: None,
                },
            );
        }
        let (index, contracts_merkle) = self.get_mutables();
        let element = index.get_mut(&contract_id).unwrap();

        element.len = memory.current_len;

        for (dirty_page, _, page_index) in memory.dirty_pages() {
            element.page_indices.insert(*page_index);
            let hash = Hash::new(dirty_page);
            element.tree.insert(*page_index as u64, hash);
        }

        let root = element.tree.root();
        let pos = position_from_contract(&contract_id);
        let int_pos = contracts_merkle.insert(pos, *root);
        element.hash = Some(*root);
        element.int_pos = Some(int_pos);
    }

    // to satisfy borrow checker
    fn get_mutables(
        &mut self,
    ) -> (&mut NewContractIndex, &mut ContractsMerkle) {
        (&mut self.index2, &mut self.contracts_merkle)
    }

    pub fn root(&self) -> Ref<Hash> {
        tracing::trace!("calculating root started");
        let ret = self.contracts_merkle.root();
        tracing::trace!("calculating root finished");
        ret
    }

    /*
    index accessors
     */

    pub fn remove_contract_index(
        &mut self,
        contract_id: &ContractId,
    ) -> Option<ContractIndexElement> {
        self.index2.contracts_mut().remove(contract_id)
    }

    pub fn insert_contract_index(
        &mut self,
        contract_id: &ContractId,
        element: ContractIndexElement,
    ) {
        self.index2.contracts_mut().insert(*contract_id, element);
    }

    pub fn index_get(
        &self,
        contract_id: &ContractId,
    ) -> Option<&ContractIndexElement> {
        let index = self.index.map(|p| unsafe { p.as_ref().unwrap() });
        match index {
            Some(p) => self
                .index2
                .get(contract_id)
                .or_else(move || p.get(contract_id)),
            None => self.index2.get(contract_id),
        }
    }

    pub fn index_contains_key(&self, contract_id: &ContractId) -> bool {
        let index = self.index.map(|p| unsafe { p.as_ref().unwrap() });
        match index {
            Some(p) => {
                self.index2.contains_key(contract_id)
                    || p.contains_key(contract_id)
            }
            None => self.index2.contains_key(contract_id),
        }
    }
}
