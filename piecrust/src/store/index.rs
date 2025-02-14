// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use crate::store::hasher::Hash;
use crate::store::tree::{PageTree, Tree};
use bytecheck::CheckBytes;
use piecrust_uplink::ContractId;
use rkyv::{Archive, Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Archive, Deserialize, Serialize)]
#[archive_attr(derive(CheckBytes))]
pub struct NewContractIndex {
    inner_contracts: BTreeMap<ContractId, ContractIndexElement>,
}

impl NewContractIndex {
    pub fn new() -> Self {
        Self {
            inner_contracts: BTreeMap::new(),
        }
    }

    pub fn remove_contract_index(
        &mut self,
        contract_id: &ContractId,
    ) -> Option<ContractIndexElement> {
        self.inner_contracts.remove(contract_id)
    }

    pub fn insert_contract_index(
        &mut self,
        contract_id: &ContractId,
        element: ContractIndexElement,
    ) {
        self.inner_contracts.insert(*contract_id, element);
    }

    pub fn get(&self, contract: &ContractId) -> Option<&ContractIndexElement> {
        self.inner_contracts.get(contract)
    }

    pub fn get_mut(
        &mut self,
        contract: &ContractId,
    ) -> Option<&mut ContractIndexElement> {
        self.inner_contracts.get_mut(contract)
    }

    pub fn contains_key(&self, contract: &ContractId) -> bool {
        self.inner_contracts.contains_key(contract)
    }

    pub fn iter(
        &self,
    ) -> impl Iterator<Item = (&ContractId, &ContractIndexElement)> {
        self.inner_contracts.iter()
    }

    pub fn contracts(&self) -> &BTreeMap<ContractId, ContractIndexElement> {
        &self.inner_contracts
    }
    pub fn contracts_mut(
        &mut self,
    ) -> &mut BTreeMap<ContractId, ContractIndexElement> {
        &mut self.inner_contracts
    }
}

impl Default for NewContractIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Archive, Deserialize, Serialize)]
#[archive_attr(derive(CheckBytes))]
pub struct ContractIndex {
    pub tree: Tree,
    pub contracts: BTreeMap<ContractId, ContractIndexElement>,
    pub contract_hints: Vec<ContractId>,
    pub maybe_base: Option<Hash>,
}

#[derive(Debug, Clone, Archive, Deserialize, Serialize)]
#[archive_attr(derive(CheckBytes))]
pub struct ContractIndexElement {
    pub tree: PageTree,
    pub len: usize,
    pub page_indices: BTreeSet<usize>,
    pub hash: Option<Hash>,
    pub int_pos: Option<u64>,
}

impl ContractIndexElement {
    pub fn new(is_64: bool) -> Self {
        Self {
            tree: PageTree::new(is_64),
            len: 0,
            page_indices: BTreeSet::new(),
            hash: None,
            int_pos: None,
        }
    }

    pub fn page_indices_and_tree(
        self,
    ) -> (impl Iterator<Item = usize>, PageTree) {
        (self.page_indices.into_iter(), self.tree)
    }

    pub fn page_indices(&self) -> &BTreeSet<usize> {
        &self.page_indices
    }

    pub fn set_len(&mut self, len: usize) {
        self.len = len;
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn set_hash(&mut self, hash: Option<Hash>) {
        self.hash = hash;
    }

    pub fn hash(&self) -> Option<Hash> {
        self.hash
    }

    pub fn set_int_pos(&mut self, int_pos: Option<u64>) {
        self.int_pos = int_pos;
    }

    pub fn int_pos(&self) -> Option<u64> {
        self.int_pos
    }

    pub fn tree(&self) -> &PageTree {
        &self.tree
    }

    pub fn insert_page_index_hash(
        &mut self,
        page_index: usize,
        page_index_u64: u64,
        page_hash: impl Into<Hash>,
    ) {
        self.page_indices.insert(page_index);
        self.tree.insert(page_index_u64, page_hash);
    }
}
