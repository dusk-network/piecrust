// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use crate::store::commit::Commit;
use crate::store::hasher::Hash;
use std::collections::btree_map::Keys;
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct CommitStore {
    commits: BTreeMap<Hash, Commit>,
}

impl CommitStore {
    pub fn new() -> Self {
        Self {
            commits: BTreeMap::new(),
        }
    }

    pub fn insert_commit(&mut self, hash: Hash, commit: Commit) {
        self.commits.insert(hash, commit);
    }

    pub fn get_commit(&self, hash: &Hash) -> Option<&Commit> {
        self.commits.get(hash)
    }

    pub fn contains_key(&self, hash: &Hash) -> bool {
        self.commits.contains_key(hash)
    }

    pub fn keys(&self) -> Keys<'_, Hash, Commit> {
        self.commits.keys()
    }

    pub fn remove_commit(&mut self, hash: &Hash) {
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
        self.commits.remove(hash);
    }
}
