// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use blake3::Hasher as Blake3Hasher;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Hash([u8; 32]);

impl Hash {
    pub const ZERO: Hash = Hash([0u8; 32]);
}

impl From<[u8; 32]> for Hash {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl AsRef<[u8]> for Hash {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct Hasher {
    state: Blake3Hasher,
}

impl Default for Hasher {
    fn default() -> Self {
        Self::new()
    }
}

impl Hasher {
    pub fn new() -> Self {
        Hasher {
            state: Blake3Hasher::new(),
        }
    }

    pub fn update(&mut self, data: impl AsRef<[u8]>) {
        self.state.update(data.as_ref());
    }

    pub fn chain_update(self, data: impl AsRef<[u8]>) -> Self {
        let mut hasher = self;
        hasher.state.update(data.as_ref());
        hasher
    }

    pub fn output(self) -> Hash {
        let hasher = self;

        let mut buf = [0u8; 32];
        buf.copy_from_slice(hasher.state.finalize().as_bytes());

        Hash::from(buf)
    }

    pub fn finalize(self) -> Hash {
        self.output()
    }

    pub fn digest(data: impl AsRef<[u8]>) -> Hash {
        let mut hasher = Hasher::new();
        hasher.update(data.as_ref());
        hasher.finalize()
    }
}
