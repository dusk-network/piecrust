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

// There are max `2^16` pages in a 32-bit memory
const P32_HEIGHT: usize = 8;
const P32_ARITY: usize = 4;

type PageTree32 = dusk_merkle::Tree<Hash, P32_HEIGHT, P32_ARITY>;

// There are max `2^26` pages in a 64-bit memory
const P64_HEIGHT: usize = 13;
const P64_ARITY: usize = 4;

type PageTree64 = dusk_merkle::Tree<Hash, P64_HEIGHT, P64_ARITY>;

// This means we have max `2^32` contracts
const C_HEIGHT: usize = 32;
const C_ARITY: usize = 2;

#[derive(Debug, Clone, Archive, Deserialize, Serialize)]
#[archive_attr(derive(CheckBytes))]
pub enum PageTree {
    Wasm32(PageTree32),
    Wasm64(PageTree64),
}

impl PageTree {
    pub fn new(is_64: bool) -> Self {
        if is_64 {
            Self::Wasm64(PageTree64::new())
        } else {
            Self::Wasm32(PageTree32::new())
        }
    }

    pub fn insert(&mut self, position: u64, item: impl Into<Hash>) {
        match self {
            Self::Wasm32(tree) => tree.insert(position, item),
            Self::Wasm64(tree) => tree.insert(position, item),
        }
    }

    pub fn root(&self) -> Ref<Hash> {
        match self {
            Self::Wasm32(tree) => tree.root(),
            Self::Wasm64(tree) => tree.root(),
        }
    }

    pub fn opening(&self, position: u64) -> Option<InnerPageOpening> {
        match self {
            Self::Wasm32(tree) => {
                let opening = tree.opening(position)?;
                Some(InnerPageOpening::Wasm32(opening))
            }
            Self::Wasm64(tree) => {
                let opening = tree.opening(position)?;
                Some(InnerPageOpening::Wasm64(opening))
            }
        }
    }
}

pub type Tree = dusk_merkle::Tree<Hash, C_HEIGHT, C_ARITY>;

#[derive(Debug, Clone, Archive, Deserialize, Serialize)]
#[archive_attr(derive(CheckBytes))]
pub struct ContractIndex {
    tree: Tree,
    contracts: BTreeMap<ContractId, ContractIndexElement>,
}

#[derive(Debug, Clone, Archive, Deserialize, Serialize)]
#[archive_attr(derive(CheckBytes))]
pub struct BaseInfo {
    pub contract_hints: Vec<ContractId>,
    pub maybe_base: Option<Hash>,
}

impl Default for BaseInfo {
    fn default() -> Self {
        Self {
            contract_hints: Vec::new(),
            maybe_base: None,
        }
    }
}

impl Default for ContractIndex {
    fn default() -> Self {
        Self {
            tree: Tree::new(),
            contracts: BTreeMap::new(),
        }
    }
}

impl ContractIndex {
    pub fn inclusion_proofs(
        mut self,
        contract_id: &ContractId,
    ) -> Option<impl Iterator<Item = (usize, PageOpening)>> {
        let contract = self.contracts.remove(contract_id)?;

        let pos = position_from_contract(contract_id);

        Some(contract.page_indices.into_iter().map(move |page_index| {
            let tree_opening = self
                .tree
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
                    tree: PageTree::new(memory.is_64()),
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

    pub fn remove_and_insert(&mut self, contract: ContractId, memory: &Memory) {
        self.contracts.remove(&contract);
        self.insert(contract, memory);
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

type Wasm32PageOpening = dusk_merkle::Opening<Hash, P32_HEIGHT, P32_ARITY>;
type Wasm64PageOpening = dusk_merkle::Opening<Hash, P64_HEIGHT, P64_ARITY>;

#[derive(Debug, Clone, Archive, Deserialize, Serialize)]
#[archive_attr(derive(CheckBytes))]
#[allow(clippy::large_enum_variant)]
pub enum InnerPageOpening {
    Wasm32(Wasm32PageOpening),
    Wasm64(Wasm64PageOpening),
}

impl InnerPageOpening {
    fn verify(&self, page: &[u8]) -> bool {
        let page_hash = Hash::new(page);

        match self {
            Self::Wasm32(opening) => opening.verify(page_hash),
            Self::Wasm64(opening) => opening.verify(page_hash),
        }
    }

    fn root(&self) -> &Hash {
        match self {
            InnerPageOpening::Wasm32(inner) => inner.root(),
            InnerPageOpening::Wasm64(inner) => inner.root(),
        }
    }
}

type TreeOpening = dusk_merkle::Opening<Hash, C_HEIGHT, C_ARITY>;

/// A Merkle opening for page in the state.
#[derive(Debug, Clone, Archive, Deserialize, Serialize)]
#[archive_attr(derive(CheckBytes))]
pub struct PageOpening {
    tree: TreeOpening,
    inner: InnerPageOpening,
}

impl PageOpening {
    /// The root of the state tree when this opening was created.
    ///
    /// This is meant to be used together with [`Session::root`] and [`verify`]
    /// to prove that a page is in the state.
    ///
    /// [`Session::root`]: crate::Session::root
    /// [`verify`]: PageOpening::verify
    pub fn root(&self) -> &Hash {
        self.tree.root()
    }

    /// Verify that the given page corresponds to the opening.
    ///
    /// To truly verify that the page is in the state, it also needs to be
    /// checked that the [`root`] of this opening is equal to the
    /// [`Session::root`].
    ///
    /// [`root`]: PageOpening::root
    /// [`Session::root`]: crate::Session::root
    pub fn verify(&self, page: &[u8]) -> bool {
        self.inner.verify(page) & self.tree.verify(*self.inner.root())
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

impl dusk_merkle::Aggregate<P32_ARITY> for Hash {
    const EMPTY_SUBTREE: Self = Hash([0; blake3::OUT_LEN]);

    fn aggregate(items: [&Self; P32_ARITY]) -> Self {
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
