// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use crate::{ContractId, Hasher};

/// Generate a [`ContractId`] address from:
/// - slice of bytes,
/// - nonce
/// - metadata
/// that is also a valid [`BlsScalar`]
pub fn gen_contract_id(
    bytes: &[u8],
    nonce: u64,
    metadata: &[u8],
) -> ContractId {
    let mut hasher = Hasher::new();
    hasher.update(bytes);
    hasher.update(nonce.to_le_bytes());
    hasher.update(metadata);
    let hash_bytes = hasher.finalize();
    ContractId::from_bytes(hash_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::{RngCore, SeedableRng};

    #[test]
    fn test_gen_contract_id() {
        let mut rng = StdRng::seed_from_u64(42);

        let mut bytes = Vec::new();
        bytes.resize(1000, 0u8);
        rng.fill_bytes(&mut bytes);

        let nonce = rng.next_u64();

        let mut version = Vec::new();
        bytes.resize(100, 0u8);
        rng.fill_bytes(&mut version);

        let contract_id =
            gen_contract_id(bytes.as_slice(), nonce, version.as_slice());

        assert_eq!(contract_id.as_bytes(), hex::decode("1b7257220fcb5617313de84e54a2f8f69ebc530c3f1207c9997f323bdd8588d3").expect("hex decoding should succeed"));
    }
}
