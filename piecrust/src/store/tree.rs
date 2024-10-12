// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::path::{Path, PathBuf};
use std::{
    cell::Ref,
    collections::{BTreeMap, BTreeSet},
};

use bytecheck::CheckBytes;
use piecrust_uplink::ContractId;
use rkyv::{Archive, Deserialize, Serialize};

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

// struct ReverseInsert {
//     pos: u64,
//     value: Option<Hash> // if Some, insert, if None, remove
// }
//

// struct Insert {
//    pos: u64,
//    value: Hash,
// }

// fn perform_inserts(maybe_commit_id: Option<Hash>, reverse_inserts: &mut
// Vec<ReverseInsert>, &merkle_tree) {     match maybe_commit_id {
//         Some(commit_id) => {
//             let maybe_base = base_of_commit(commit_id)
//             perform_inserts(maybe_base, reverse_inserts, merkle_tree);
//             let inserts = read_inserts_for_commit_id(&commit_id);
//             for every insert {
//                 let reverse_insert = apply_insert(insert, &merkle_tree);
//                 reverse_inserts.push(reverse_insert);
//             }
//         },
//         None => {
//            return;
//         }
//     }
//

// fn perform_reversals(reverse_inserts: &Vec<ReverseInsert>, &merkle_tree) {
//     from end to the beginning of reverse_inserts, for every reverse_insert do
// {         apply_reverse_insert(reverse_insert, merkle_tree) // meaning either
// insert or removal     }
// }

// struct CommitInserts
//
// data members:
//    inserts: Vec<Insert>
//
// methods:
//    fn calc_root(&self, &mut central_merkle_tree, commit_id) -> root {
//        let maybe_base = base_of_commit(commit_id);
//        let mut reverse_inserts = Vec::<ReverseInsert>::new();
//        perform_inserts(maybe_base, &mut reverse_inserts,
// &central_merkle_tree);        let root =
// calc_the_actual_root(&central_merkle_tree);        perform_reversals(&
// reverse_inserts, &central_merkle_tree);        root
//    }
//    fn finalize(&self, &mut central_merkle_tree, commit_id) {
//        let maybe_base = base_of_commit(commit_id);
//        let mut reverse_inserts = Vec::<ReverseInsert>::new();
//        perform_inserts(maybe_base, &mut reverse_inserts);
//        save central_merkle_tree to disk
//        remove yourself on Disk or make sure it is being done as part of a
// greater           finalization of commit commit_id
//    }
//    fn read_from_file_repr() -> io::Result<Self> {
//    }
//    fn remove_file_repr(&self) -> io::Result<()>{
//    }

// this should only contain a collection of merkle tree inserts (CommitInserts)
// this should really be a class which is able to calc root
// and finalize
#[derive(Debug, Clone, Archive, Deserialize, Serialize)]
#[archive_attr(derive(CheckBytes))]
pub struct NewContractIndex {
    inner_contracts: BTreeMap<ContractId, ContractIndexElement>,
    path: String,
}

#[derive(Debug, Clone, Archive, Deserialize, Serialize)]
#[archive_attr(derive(CheckBytes))]
pub struct ContractsMerkle {
    pub tree: Tree,
}

impl Default for ContractsMerkle {
    fn default() -> Self {
        Self { tree: Tree::new() }
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

#[derive(Debug, Clone, Default, Archive, Deserialize, Serialize)]
#[archive_attr(derive(CheckBytes))]
pub struct BaseInfo {
    pub contract_hints: Vec<ContractId>,
    pub maybe_base: Option<Hash>,
}

#[derive(Debug, Clone, Archive, Deserialize, Serialize)]
#[archive_attr(derive(CheckBytes))]
pub struct ContractIndexElement {
    pub tree: PageTree,
    pub len: usize,
    pub page_indices: BTreeSet<usize>,
}

impl NewContractIndex {
    pub fn new<P: AsRef<Path>>(dir: P) -> Self {
        Self {
            inner_contracts: BTreeMap::new(),
            // path: dir.as_ref().to_path_buf(),
            path: dir.as_ref().to_string_lossy().to_string(), /* todo: after removal of Archive derivation to NewContractIndex, change the type to PathBuf */
        }
    }

    pub fn path(&self) -> PathBuf {
        // todo: after removal of Archive derivation to NewContractIndex, change
        // the implementation
        PathBuf::from(self.path.clone())
    }

    pub fn remove_contract_index(
        &mut self,
        contract_id: &ContractId,
        _maybe_commit_id: Option<Hash>,
        // _path: impl AsRef<Path>,
    ) -> Option<ContractIndexElement> {
        self.inner_contracts.remove(contract_id)
    }

    pub fn insert_contract_index(
        &mut self,
        contract_id: ContractId,
        element: ContractIndexElement,
        _maybe_commit_id: Option<Hash>,
    ) {
        self.inner_contracts.insert(contract_id, element);
    }

    pub fn get(
        &self,
        contract: &ContractId,
        _maybe_commit_id: Option<Hash>,
    ) -> Option<&ContractIndexElement> {
        self.inner_contracts.get(contract)
    }

    pub fn get_mut(
        &mut self,
        contract: &ContractId,
        _maybe_commit_id: Option<Hash>,
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
