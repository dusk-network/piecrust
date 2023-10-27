// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::{
    cell::Ref,
    collections::{BTreeMap, BTreeSet},
};

use bytecheck::CheckBytes;
use piecrust_uplink::ContractId;
use rkyv::{Archive, Deserialize, Serialize};

use crate::store::memory::Memory;

// There are max `2^26` pages in a memory
const P_HEIGHT: usize = 13;
const P_ARITY: usize = 4;

pub type PageTree = dusk_merkle::Tree<Hash, P_HEIGHT, P_ARITY>;

// This means we have max `2^32` contracts
const C_HEIGHT: usize = 32;
const C_ARITY: usize = 2;

pub type Tree = dusk_merkle::Tree<Hash, C_HEIGHT, C_ARITY>;

#[derive(Debug, Clone, Archive, Deserialize, Serialize)]
#[archive_attr(derive(CheckBytes))]
pub struct ContractIndex {
    tree: Tree,
    contracts: BTreeMap<ContractId, ContractIndexElement>,
}

impl Default for ContractIndex {
    fn default() -> Self {
        Self {
            tree: Tree::new(),
            contracts: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Archive, Deserialize, Serialize)]
#[archive_attr(derive(CheckBytes))]
pub struct ContractIndexElement {
    pub tree: PageTree,
    pub len: usize,
    pub page_indices: BTreeSet<usize>,
}

impl ContractIndex {
    pub fn insert(&mut self, contract: ContractId, memory: &Memory) {
        if self.contracts.get(&contract).is_none() {
            self.contracts.insert(
                contract,
                ContractIndexElement {
                    tree: PageTree::new(),
                    len: 0,
                    page_indices: BTreeSet::new(),
                },
            );
        }
        let element = self.contracts.get_mut(&contract).unwrap();

        element.len = memory.current_len;

        for (dirty_page, _, page_index) in memory.dirty_pages() {
            element.page_indices.insert(*page_index);
            let hash = Hash::new(dirty_page);
            element.tree.insert(*page_index as u64, hash);
        }

        self.tree
            .insert(position_from_contract(&contract), *element.tree.root());
    }

    pub fn root(&self) -> Ref<Hash> {
        self.tree.root()
    }

    pub fn get(&self, contract: &ContractId) -> Option<&ContractIndexElement> {
        self.contracts.get(contract)
    }

    pub fn contains_key(&self, contract: &ContractId) -> bool {
        self.contracts.contains_key(contract)
    }

    pub fn iter(
        &self,
    ) -> impl Iterator<Item = (&ContractId, &ContractIndexElement)> {
        self.contracts.iter()
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Archive,
    Deserialize,
    Serialize,
    CheckBytes,
)]
#[archive(as = "Self")]
pub struct Hash([u8; blake3::OUT_LEN]);

impl Hash {
    pub fn new(bytes: &[u8]) -> Self {
        let mut hasher = Hasher::new();
        hasher.update(bytes);
        hasher.finalize()
    }

    pub fn as_bytes(&self) -> &[u8; blake3::OUT_LEN] {
        &self.0
    }
}

impl From<Hash> for [u8; blake3::OUT_LEN] {
    fn from(hash: Hash) -> Self {
        hash.0
    }
}

impl From<[u8; blake3::OUT_LEN]> for Hash {
    fn from(bytes: [u8; blake3::OUT_LEN]) -> Self {
        Self(bytes)
    }
}

impl AsRef<[u8]> for Hash {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl dusk_merkle::Aggregate<C_ARITY> for Hash {
    const EMPTY_SUBTREE: Self = Hash([0; blake3::OUT_LEN]);

    fn aggregate(items: [&Self; C_ARITY]) -> Self {
        let mut hasher = Hasher::new();
        for item in items {
            hasher.update(item.as_bytes());
        }
        hasher.finalize()
    }
}

impl dusk_merkle::Aggregate<P_ARITY> for Hash {
    const EMPTY_SUBTREE: Self = Hash([0; blake3::OUT_LEN]);

    fn aggregate(items: [&Self; P_ARITY]) -> Self {
        let mut hasher = Hasher::new();
        for item in items {
            hasher.update(item.as_bytes());
        }
        hasher.finalize()
    }
}

#[derive(Debug, Clone)]
pub struct Hasher(blake3::Hasher);

impl Hasher {
    #[inline(always)]
    pub fn new() -> Self {
        Self(blake3::Hasher::new())
    }

    #[inline(always)]
    pub fn update(&mut self, input: &[u8]) -> &mut Self {
        self.0.update(input);
        self
    }

    #[inline(always)]
    pub fn finalize(&self) -> Hash {
        let hash = self.0.finalize();
        Hash(hash.into())
    }
}

/// Returns the position of a `contract` in the tree  given its ID. The position
/// is computed by dividing the 32-byte id into 8 4-byte slices, which are then
/// summed up (`u32::wrapping_add`).
///
/// # SAFETY:
/// Since we're mapping from 32 bytes (256-bit) to 4 bytes it is possible to
/// have collisions between different contract IDs. To prevent filling the same
/// position in the tree with different contracts we check for collisions before
/// inserting a new contract. See [`deploy`] for details.
///
/// [`deploy`]: crate::store::ContractSession::deploy
pub fn position_from_contract(contract: &ContractId) -> u64 {
    let pos = contract
        .as_bytes()
        .chunks(4)
        .map(|chunk| {
            let mut bytes = [0; 4];
            bytes.copy_from_slice(chunk);
            u32::from_le_bytes(bytes)
        })
        .fold(0, u32::wrapping_add);

    pos as u64
}
