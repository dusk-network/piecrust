// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::{cell::Ref, collections::BTreeMap};

use crate::store::hasher::Hash;
use crate::store::treepos::TreePos;
use bytecheck::CheckBytes;
use piecrust_uplink::ContractId;
use rkyv::{Archive, Deserialize, Serialize};

// There are max `2^16` pages in a 32-bit memory
const P32_HEIGHT: usize = 8;
pub const P32_ARITY: usize = 4;

type PageTree32 = dusk_merkle::Tree<Hash, P32_HEIGHT, P32_ARITY>;

// There are max `2^26` pages in a 64-bit memory
const P64_HEIGHT: usize = 13;
const P64_ARITY: usize = 4;

type PageTree64 = dusk_merkle::Tree<Hash, P64_HEIGHT, P64_ARITY>;

// This means we have max `2^32` contracts
const C_HEIGHT: usize = 32;
pub const C_ARITY: usize = 2;

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
pub struct ContractsMerkle {
    inner_tree: Tree,
    dict: BTreeMap<u64, u64>,
    tree_pos: TreePos,
}

impl Default for ContractsMerkle {
    fn default() -> Self {
        Self {
            inner_tree: Tree::new(),
            dict: BTreeMap::new(),
            tree_pos: TreePos::default(),
        }
    }
}

impl ContractsMerkle {
    pub fn insert(&mut self, pos: u64, hash: Hash) -> u64 {
        let new_pos = match self.dict.get(&pos) {
            None => {
                let new_pos = (self.dict.len() + 1) as u64;
                self.dict.insert(pos, new_pos);
                new_pos
            }
            Some(p) => *p,
        };
        self.inner_tree.insert(new_pos, hash);
        self.tree_pos.insert(new_pos as u32, (hash, pos));
        new_pos
    }

    pub fn insert_with_int_pos(&mut self, pos: u64, int_pos: u64, hash: Hash) {
        self.dict.insert(pos, int_pos);
        self.inner_tree.insert(int_pos, hash);
        self.tree_pos.insert(int_pos as u32, (hash, pos));
    }

    pub fn opening(&self, pos: u64) -> Option<TreeOpening> {
        let new_pos = self.dict.get(&pos)?;
        self.inner_tree.opening(*new_pos)
    }

    pub fn root(&self) -> Ref<Hash> {
        self.inner_tree.root()
    }

    pub fn tree_pos(&self) -> &TreePos {
        &self.tree_pos
    }

    pub fn len(&self) -> u64 {
        self.inner_tree.len()
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
    pub tree: TreeOpening,
    pub inner: InnerPageOpening,
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
