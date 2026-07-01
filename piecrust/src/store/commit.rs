// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

pub mod finalizer;
pub mod reader;
pub mod remover;
pub mod writer;

use std::cell::Ref;
use std::ops::Deref;
use std::sync::{Arc, Mutex, MutexGuard};

use piecrust_uplink::ContractId;
use tracing::debug;

use crate::PageOpening;
use crate::store::Memory;
use crate::store::commit_store::{CommitStore, ElementOwner};
use crate::store::hasher::Hash;
use crate::store::index::{ContractIndexElement, NewContractIndex};
use crate::store::tree::{ContractsMerkle, position_from_contract};

#[derive(Debug, Clone)]
pub(crate) struct Commit {
    index: NewContractIndex,
    contracts_merkle: ContractsMerkle,
    maybe_hash: Option<Hash>,
    commit_store: Option<Arc<Mutex<CommitStore>>>,
    base: Option<Hash>,
}

/// A reference to a local or store-backed contract index element.
pub(crate) enum ContractIndexElementRef<'a> {
    /// A reference to an element in this `Commit`'s `index` map.
    Local(&'a ContractIndexElement),
    /// A reference to an element in the `CommitStore` while its guard is held.
    Store {
        /// Keeps inherited entries alive while callers read through this ref.
        guard: MutexGuard<'a, CommitStore>,
        /// Identifies which store index to read under the guard.
        owner: ElementOwner,
        /// Identifies the specific contract entry under the guarded owner.
        contract_id: ContractId,
    },
}

impl Deref for ContractIndexElementRef<'_> {
    type Target = ContractIndexElement;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Local(element) => element,
            Self::Store {
                guard,
                owner,
                contract_id,
            } => guard.get_element(*owner, contract_id).expect(
                "index element must exist while the store guard is held",
            ),
        }
    }
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

    pub fn insert(&mut self, contract_id: ContractId, memory: &Memory) {
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
        let (element, contracts_merkle) = (
            self.index
                .get_mut(&contract_id)
                .expect("commit insertion must create a local index element"),
            &mut self.contracts_merkle,
        );

        element.set_len(memory.current_len());

        debug!("Check dirty pages for {contract_id}");
        for (dirty_page, clean, page_index) in memory.dirty_pages() {
            let hash = Hash::new(dirty_page);
            let clean = Hash::new(clean);
            // TODO: re-enable skipping of unchanged pages behind an env var to
            //       preserve old behavior
            //
            // if hash == clean {
            //     debug!(
            //         msg = "SKIPPING page",
            //         page_index,
            //         contract_id = hex::encode(&contract_id.as_bytes()[0..8]),
            //         dirty = hex::encode(hash.as_bytes()),
            //         clean = hex::encode(clean.as_bytes())
            //     );
            //     continue;
            // }
            debug!(
                msg = "insert page",
                page_index,
                contract_id = hex::encode(&contract_id.as_bytes()[0..8]),
                dirty = hex::encode(hash.as_bytes()),
                clean = hex::encode(clean.as_bytes())
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
    }

    pub fn remove_and_insert(&mut self, contract: ContractId, memory: &Memory) {
        self.index.remove_contract_index(&contract);
        self.insert(contract, memory);
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
                if let Some(mel) = commit_store.get_from_main_index(c)
                    && mel.hash() == Some(h)
                {
                    to_remove.push(*c)
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

    pub fn root(&self) -> Ref<'_, Hash> {
        tracing::trace!("calculating root started");
        let ret = self.contracts_merkle.root();
        tracing::trace!("calculating root finished");
        ret
    }

    pub fn index_get(
        &self,
        contract_id: &ContractId,
    ) -> Option<ContractIndexElementRef<'_>> {
        if let Some(e) = self.index.get(contract_id) {
            return Some(ContractIndexElementRef::Local(e));
        }

        Hulk::deep_index_get(
            *contract_id,
            self.commit_store.as_ref(),
            self.base,
        )
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
}

#[derive(Debug, Clone)]
pub(crate) struct Hulk;

impl Hulk {
    pub fn deep_index_get<'a>(
        contract_id: ContractId,
        commit_store: Option<&'a Arc<Mutex<CommitStore>>>,
        base: Option<Hash>,
    ) -> Option<ContractIndexElementRef<'a>> {
        let mut base = base?;
        let commit_store = commit_store?;
        let commit_store = commit_store.lock().unwrap();
        loop {
            let (maybe_owner, commit_base) =
                commit_store.get_element_owner_and_base(&base, &contract_id);
            if let Some(owner) = maybe_owner {
                return Some(ContractIndexElementRef::Store {
                    guard: commit_store,
                    owner,
                    contract_id,
                });
            }
            base = commit_base?;
        }
    }
}

#[cfg(all(test, miri))]
mod tests {
    use std::sync::TryLockError;

    use super::*;

    fn remove_commit_if_possible(
        commit_store: &Arc<Mutex<CommitStore>>,
        hash: &Hash,
    ) -> bool {
        match commit_store.try_lock() {
            Ok(mut commit_store) => {
                commit_store.remove_commit(hash, false);
                true
            }
            Err(TryLockError::WouldBlock) => false,
            Err(TryLockError::Poisoned(_)) => {
                panic!("commit store mutex should not be poisoned")
            }
        }
    }

    #[test]
    fn test_no_dangling_commit_index_reference() {
        let commit_store = Arc::new(Mutex::new(CommitStore::new()));
        let contract_id = ContractId::from_bytes([1; 32]);
        let ancestor_hash = Hash::new(b"ancestor");

        let mut ancestor = Commit::new(&commit_store, None);
        ancestor.index.insert_contract_index(
            &contract_id,
            ContractIndexElement::new(false),
        );

        commit_store
            .lock()
            .unwrap()
            .insert_commit(ancestor_hash, ancestor);

        let descendant = Commit::new(&commit_store, Some(ancestor_hash));
        let element = descendant
            .index_get(&contract_id)
            .expect("ancestor should contain the element");

        let removed = remove_commit_if_possible(&commit_store, &ancestor_hash);

        // Old code removes the ancestor before this read, so Miri should reject
        // the stale reference. Fixed code keeps the store locked while
        // `element` is alive, so the attempted removal is deferred.
        let _ = element.len();

        if !removed {
            drop(element);
            commit_store
                .lock()
                .unwrap()
                .remove_commit(&ancestor_hash, false);
        }
    }
}
