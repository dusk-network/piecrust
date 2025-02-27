// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{contract_bytecode, ContractData, Error, SessionData, VM};

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;

#[tokio::test(flavor = "multi_thread")]
pub async fn merkle_root() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let id = session.deploy(
        contract_bytecode!("merkle"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    // (measured) minimum gas to pass - insertion in a merkle tree is
    // "expensive".
    const GAS_LIMIT: u64 = 147456;

    let empty_root = [0u8; 32];
    let root: [u8; 32] = session
        .call(id, "root", &(), GAS_LIMIT)
        .expect("root query should succeed")
        .data;

    assert_eq!(root, empty_root, "The root should be empty value");

    let leaves: [u64; 8] = [42, 0xbeef, 0xf00, 0xba5, 314, 7297, 1, 0];
    let mut roots = [[0u8; 32]; 8];

    roots
        .iter_mut()
        .zip(leaves)
        .enumerate()
        .for_each(|(i, (root, leaf))| {
            session
                .call::<_, ()>(id, "insert", &(i as u64, leaf), LIMIT)
                .expect("tree insertion should succeed");

            *root = session
                .call(id, "root", &(), GAS_LIMIT)
                .expect("root query should succeed")
                .data;
        });

    // All roots are different from each other
    for (i, lr) in roots.iter().enumerate() {
        for rr in roots
            .iter()
            .enumerate()
            .filter_map(|(j, root)| (i != j).then_some(root))
        {
            assert_ne!(lr, rr, "roots should be different");
        }
    }

    Ok(())
}
