// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use crate::store::commit::Commit;
use crate::store::hasher::Hash;
use crate::store::index::{ContractIndexElement, NewContractIndex};
use piecrust_uplink::ContractId;
use std::collections::btree_map::Keys;
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct CommitStore {
    commits: BTreeMap<Hash, Commit>,
    main_index: NewContractIndex,
}

impl CommitStore {
    pub fn new() -> Self {
        Self {
            commits: BTreeMap::new(),
            main_index: NewContractIndex::new(),
        }
    }

    pub fn insert_commit(&mut self, hash: Hash, commit: Commit) {
        self.commits.insert(hash, commit);
    }

    pub fn get_commit(&self, hash: &Hash) -> Option<&Commit> {
        self.commits.get(hash)
    }

    pub fn get_element_and_base(
        &self,
        hash: &Hash,
        contract_id: &ContractId,
    ) -> (Option<*const ContractIndexElement>, Option<Hash>) {
        match self.commits.get(hash) {
            Some(commit) => {
                let e = commit.index.get(contract_id);
                (e.map(|a| a as *const ContractIndexElement), commit.base)
            }
            None => {
                let e = self.main_index.get(contract_id);
                (e.map(|a| a as *const ContractIndexElement), None)
            }
        }
    }

    pub fn get_element_and_base_mut(
        &mut self,
        hash: &Hash,
        contract_id: &ContractId,
    ) -> (Option<*mut ContractIndexElement>, Option<Hash>) {
        match self.commits.get_mut(hash) {
            Some(commit) => {
                let e = commit.index.get_mut(contract_id);
                (e.map(|a| a as *mut ContractIndexElement), commit.base)
            }
            None => {
                let e = self.main_index.get_mut(contract_id);
                (e.map(|a| a as *mut ContractIndexElement), None)
            }
        }
    }

    pub fn contains_key(&self, hash: &Hash) -> bool {
        self.commits.contains_key(hash)
    }

    pub fn keys(&self) -> Keys<'_, Hash, Commit> {
        self.commits.keys()
    }

    pub fn remove_commit(&mut self, hash: &Hash, deep: bool) {
        if deep {
            let mut elements_to_remove = BTreeMap::new();
            if let Some(removed_commit) = self.commits.get(hash) {
                for (contract_id, element) in
                    removed_commit.index.contracts().iter()
                {
                    if let Some(h) = element.hash() {
                        elements_to_remove.insert(*contract_id, h);
                    }
                }
            }
            // other commits should not keep finalized elements
            for (h, commit) in self.commits.iter_mut() {
                if h == hash {
                    continue;
                }
                for (c, hh) in elements_to_remove.iter() {
                    if let Some(el) = commit.index.get(c) {
                        if let Some(el_hash) = el.hash() {
                            if el_hash == *hh {
                                commit.index.remove_contract_index(c);
                            }
                        }
                    }
                }
            }
        }
        self.commits.remove(hash);
    }

    pub fn insert_main_index(
        &mut self,
        contract_id: &ContractId,
        element: ContractIndexElement,
    ) {
        self.main_index.insert_contract_index(contract_id, element);
    }
}
