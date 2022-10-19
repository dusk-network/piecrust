// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use crate::commit::Hashable;

pub struct Merkle {}

const MERKLE_HASH_LEN: usize = 32;

impl Merkle {
    pub fn merkle<H>(vec: &mut [H]) -> H
    where
        H: Hashable + From<[u8; MERKLE_HASH_LEN]> + Copy,
    {
        let mut vec_len = vec.len();
        while vec_len > 1 {
            vec_len = Self::merkle_step(vec, vec_len);
        }
        if vec_len == 0 {
            H::uninitialized()
        } else {
            vec[0]
        }
    }

    fn merkle_step<H>(vec: &mut [H], vec_len: usize) -> usize
    where
        H: Hashable + From<[u8; MERKLE_HASH_LEN]> + Copy,
    {
        let len = vec_len + vec_len % 2;
        for i in 0..len / 2 {
            let mut pair = [0u8; MERKLE_HASH_LEN * 2];
            pair.as_mut_slice()[..MERKLE_HASH_LEN]
                .copy_from_slice(vec[2 * i].as_slice());
            let index = if (2 * i + 1) == vec_len {
                2 * i
            } else {
                2 * i + 1
            };
            pair.as_mut_slice()[MERKLE_HASH_LEN..]
                .copy_from_slice(vec[index].as_slice());
            vec[i] = H::from(*blake3::hash(pair.as_slice()).as_bytes());
        }
        len / 2
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::commit::{ModuleCommitId, COMMIT_ID_BYTES};

    #[test]
    fn empty_merkle() {
        let mut v: Vec<ModuleCommitId> = vec![];
        assert_eq!(Merkle::merkle(&mut v), ModuleCommitId::uninitialized());
    }

    #[test]
    fn one_element_merkle() {
        let mut v = vec![];
        v.push(ModuleCommitId::from([1u8; COMMIT_ID_BYTES]));
        assert_eq!(
            Merkle::merkle(&mut v),
            ModuleCommitId::from([1u8; COMMIT_ID_BYTES])
        );
    }

    #[test]
    fn two_elements_merkle() {
        let mut v = vec![];
        for _ in 0..2 {
            v.push(ModuleCommitId::from([1u8; COMMIT_ID_BYTES]));
        }
        let expected_merkle =
            *blake3::hash([1u8; COMMIT_ID_BYTES * 2].as_slice()).as_bytes();
        assert_eq!(
            Merkle::merkle(&mut v),
            ModuleCommitId::from(expected_merkle)
        );
    }

    #[test]
    fn three_elements_merkle() {
        let mut v = vec![];
        for i in 0..3 {
            v.push(ModuleCommitId::from([i as u8; COMMIT_ID_BYTES]));
        }
        let mut expected_input = [0u8; COMMIT_ID_BYTES * 2];
        for i in 0..expected_input.len() {
            expected_input[i] = (i / COMMIT_ID_BYTES) as u8;
        }
        let expected_merkle_1 =
            *blake3::hash(expected_input.as_slice()).as_bytes();

        for i in 0..expected_input.len() {
            expected_input[i] = 2;
        }
        let expected_merkle_2 =
            *blake3::hash(expected_input.as_slice()).as_bytes();

        expected_input.as_mut_slice()[..COMMIT_ID_BYTES]
            .copy_from_slice(expected_merkle_1.as_slice());
        expected_input.as_mut_slice()[COMMIT_ID_BYTES..]
            .copy_from_slice(expected_merkle_2.as_slice());
        let expected_merkle =
            *blake3::hash(expected_input.as_slice()).as_bytes();

        assert_eq!(
            Merkle::merkle(&mut v),
            ModuleCommitId::from(expected_merkle)
        );
    }

    #[test]
    fn four_elements_merkle() {
        let mut v = vec![];
        for i in 0..4 {
            v.push(ModuleCommitId::from([i as u8; COMMIT_ID_BYTES]));
        }
        let mut expected_input = [0u8; COMMIT_ID_BYTES * 2];
        for i in 0..expected_input.len() {
            expected_input[i] = (i / COMMIT_ID_BYTES) as u8;
        }
        let expected_merkle_1 =
            *blake3::hash(expected_input.as_slice()).as_bytes();

        for i in 0..expected_input.len() {
            expected_input[i] = (i / COMMIT_ID_BYTES) as u8 + 2;
        }
        let expected_merkle_2 =
            *blake3::hash(expected_input.as_slice()).as_bytes();

        expected_input.as_mut_slice()[..COMMIT_ID_BYTES]
            .copy_from_slice(expected_merkle_1.as_slice());
        expected_input.as_mut_slice()[COMMIT_ID_BYTES..]
            .copy_from_slice(expected_merkle_2.as_slice());
        let expected_merkle =
            *blake3::hash(expected_input.as_slice()).as_bytes();

        assert_eq!(
            Merkle::merkle(&mut v),
            ModuleCommitId::from(expected_merkle)
        );
    }
}
