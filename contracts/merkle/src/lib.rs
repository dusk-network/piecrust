// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Contract allowing for insertion of leaves into a merkle tree and retrieval
//! of the root.

#![feature(arbitrary_self_types)]
#![no_std]

use piecrust_uplink as uplink;

use blake3::Hasher;
use dusk_merkle::{Aggregate, Tree};

const H: usize = 17;
const A: usize = 4;

#[derive(Clone, Copy)]
struct Hash([u8; 32]);

impl<I: Into<[u8; 32]>> From<I> for Hash {
    fn from(bytes: I) -> Self {
        Self(bytes.into())
    }
}

impl Aggregate<H, A> for Hash {
    const EMPTY_SUBTREES: [Self; H] = [Hash([0; 32]); H];

    fn aggregate<'a, I>(items: I) -> Self
    where
        Self: 'a,
        I: Iterator<Item = &'a Self>,
    {
        let mut hasher = Hasher::new();
        for item in items {
            hasher.update(&item.0);
        }
        Self::from(hasher.finalize())
    }
}

/// Struct that describes the state of the merkle contract. It contains a Merkle
/// tree of height `H` and arity `A`.
pub struct Merkle {
    tree: Tree<Hash, H, A>,
}

impl Merkle {
    const fn new() -> Self {
        Self { tree: Tree::new() }
    }
}

/// State of the merkle contract
static mut STATE: Merkle = Merkle::new();

impl Merkle {
    fn insert_u64(&mut self, pos: u64, int: u64) {
        let hash = blake3::hash(&int.to_le_bytes());
        self.tree.insert(pos, hash);
    }

    fn root(&self) -> [u8; 32] {
        let root = self.tree.root().0;
        uplink::debug!("{:?}", root);
        root
    }
}

/// Expose `Merkle::insert()` to the host
#[no_mangle]
unsafe fn insert(a: u32) -> u32 {
    uplink::wrap_call(a, |(pos, int)| STATE.insert_u64(pos, int))
}

/// Expose `Merkle::root()` to the host
#[no_mangle]
unsafe fn root(a: u32) -> u32 {
    uplink::wrap_call(a, |_: ()| STATE.root())
}
