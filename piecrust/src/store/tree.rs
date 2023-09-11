// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::collections::{BTreeMap, BTreeSet};

use bytecheck::CheckBytes;
use crumbles::PAGE_SIZE;
use piecrust_uplink::ContractId;
use rkyv::{Archive, Deserialize, Serialize};

use crate::store::Memory;

// There are `2^16` pages in a memory
const P_HEIGHT: usize = 16;
const P_ARITY: usize = 2;

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
    pub offsets: BTreeSet<usize>,
}

impl ContractIndex {
    pub fn insert(&mut self, contract: ContractId, memory: &Memory) {
        if self.contracts.get(&contract).is_none() {
            self.contracts.insert(
                contract,
                ContractIndexElement {
                    tree: PageTree::new(),
                    len: 0,
                    offsets: BTreeSet::new(),
                },
            );
        }
        let element = self.contracts.get_mut(&contract).unwrap();

        let memory = memory.read();
        let memory_inner = memory.inner;

        element.len = memory_inner.def.current_length;

        for (dirty_page, _, page_offset) in memory_inner.mmap.dirty_pages() {
            element.offsets.insert(page_offset);

            let hash = Hash::new(dirty_page);
            let page_pos = page_offset / PAGE_SIZE;

            element.tree.insert(page_pos as u64, hash);
        }

        self.tree
            .insert(position_from_contract(&contract), *element.tree.root());
    }

    pub fn root(&self) -> &Hash {
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

impl dusk_merkle::Aggregate<C_HEIGHT, C_ARITY> for Hash {
    const EMPTY_SUBTREES: [Hash; C_HEIGHT] = C_EMPTY_SUBTREES;

    fn aggregate<'a, I>(items: I) -> Self
    where
        Self: 'a,
        I: Iterator<Item = &'a Self>,
    {
        let mut hasher = Hasher::new();
        for item in items {
            hasher.update(item.as_bytes());
        }
        hasher.finalize()
    }
}

impl dusk_merkle::Aggregate<P_HEIGHT, P_ARITY> for Hash {
    const EMPTY_SUBTREES: [Hash; P_HEIGHT] = P_EMPTY_SUBTREES;

    fn aggregate<'a, I>(items: I) -> Self
    where
        Self: 'a,
        I: Iterator<Item = &'a Self>,
    {
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

const P_EMPTY_SUBTREES: [Hash; P_HEIGHT] = subtrees();
const C_EMPTY_SUBTREES: [Hash; C_HEIGHT] = subtrees();

const fn subtrees<const N: usize>() -> [Hash; N] {
    let mut subtrees = [Hash([0; blake3::OUT_LEN]); N];

    let mut i = 0;
    while i < N {
        subtrees[i] = EMPTY_SUBTREES[i];
        i += 1;
    }

    subtrees
}

const EMPTY_SUBTREES: [Hash; C_HEIGHT] = [
    h2h("452b8b91767b270a61db2159db2dd6d411ce560f4a34230d70928950fdda3a88"),
    h2h("f588290692a4d73ba016dee67a00e928f38e381b8eb5ca8c01d8b8c9bee32211"),
    h2h("0b69ae637dbf590e15b9e06b4195341087cb3f60e95a43a1a8c459a5d321b1ff"),
    h2h("4087415c8d818403a4b52dcb9fc1c100ac72c3b52dba28181924cc394f39d873"),
    h2h("d426e95515107709df948c05a45cf9257c03510fe89ed6e81c0f34bd02c67a1f"),
    h2h("f71910e10293859199e9ce0a9eb553d9a332b9931b0516842ff4528143493789"),
    h2h("19af8e0debcf1f9ed745a87e264e0930ac0163066b4a78bba42660252bf888d0"),
    h2h("82b948b894a90ff6b6b5ff728e2f4e4d5442d40e52a8920ef4fb6922469032a1"),
    h2h("7a6b33825bf9cb46f03a4db60821f811679b3b2ea0bc62e92b2c70a55b207be7"),
    h2h("ba58322ab8cdb81e2d3e5f48c56d016c94fa03d37559e1fe2f6ce5e1947c1b11"),
    h2h("2a6f776446db67a81008cc0d39a59c65456dfc0de7119158dcb593fceda81def"),
    h2h("fcc45ccc66aef0862029fc21785cbc1de4f15f4a1de5f6ed2458b1061093de18"),
    h2h("c20f6e2e047be3bccbfbc9a66352ee30a6b664a03da6df415fa2fde67e174c60"),
    h2h("a0abd3f2694a3518379cd1df96701eb10d94f1025732796f28c1f6eb7a197282"),
    h2h("ee20a233662e859c2af6a3feff6f1de5cea6d3421074d7a5fd2a472ba8083f51"),
    h2h("d47a16c4ebfebe96fe29a5df123992c17f092f990cf4403a3f5b9c3d86219105"),
    h2h("71c0c670191c8b1b7389d83b6da3e6a00f467ccc477ef0eabfbee4ab7b4b0087"),
    h2h("136ce719e0a7865f0f39715a85e97a44284dc20fc86edff441b52d523f9c8aa5"),
    h2h("5149df3556905068c59c3e5a35691b77dc31f411106802c29b4924009b95d953"),
    h2h("b5b17224a6b2965cb162319dbc0a9ff87d3eee8f84fcc35e568fcf034e257601"),
    h2h("1f23b1ebdb969ed8f77520d17e5c49b09c2becb73f65807ce548fac20705fb99"),
    h2h("31bc05ea5dc0009352ddb2af607fddbf7605a314411aa77b18b1e00caef0109a"),
    h2h("e895bc877f2c61c066e5e742b5e6e3b054e0e50c2f97cf6b1ff3d70830964765"),
    h2h("5002d9b6c51486682cf6d109ebcacad9662bbaca1ce80fcdc38d4b5a3bde4f7f"),
    h2h("3fe9023a11950dcbd3be3da7023f006369832ee531fbe25b4068bb9d3fc86878"),
    h2h("8505d1e1aaac1592fe422ab50ed6d7e08a8f4ec1935be058f7fba446ffbf6868"),
    h2h("513fef2ba48aadea5347054c866e9a1101475a9616badbdca1e8d38e76fa01a7"),
    h2h("b1cbaba537ce45006efc736dbaa54d325cd9d5b7c03a796cc50b91ba06216fbf"),
    h2h("d5e03be0716d41ef6cb9a2e7a761fba03c7e2df1e3c9c5a73560ed0f82900795"),
    h2h("cc84fcaddbb73c6dc3803fb015a27e8aca4cb5783c1da7f0f1af353311b447d0"),
    h2h("0ebcb81ff4a62dcd965ea581fffead4dbecf308ac7400eb13dbad6d4c253e3c0"),
    h2h("88ade4a900cb56d9dbbef5e4f4e15cb0bbc40f2fcbabd916926b6b1481acfdc4"),
];

const fn h2h(s: &str) -> Hash {
    use const_decoder::Decoder;
    let bytes = Decoder::Hex.decode(s.as_bytes());
    Hash(bytes)
}
