// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{contract_bytecode, ContractData, Error, SessionData, VM};
use piecrust_uplink::ContractId;

const LIMIT: u64 = 1_000_000;

#[test]
fn metadata() -> Result<(), Error> {
    const EXPECTED_OWNER: [u8; 33] = [3u8; 33];

    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let id = session.deploy(
        contract_bytecode!("metadata"),
        ContractData::builder(EXPECTED_OWNER),
        LIMIT,
    )?;

    // owner should be available after deployment
    let owner = session
        .call::<_, [u8; 33]>(id, "read_owner", &(), LIMIT)?
        .data;
    let self_id = session
        .call::<_, ContractId>(id, "read_id", &(), LIMIT)?
        .data;
    assert_eq!(owner, EXPECTED_OWNER);
    assert_eq!(self_id, id);

    // owner should live across session boundaries
    let commit_id = session.commit()?;
    let mut session = vm.session(SessionData::builder().base(commit_id))?;
    let owner = session
        .call::<_, [u8; 33]>(id, "read_owner", &(), LIMIT)?
        .data;
    let self_id = session
        .call::<_, ContractId>(id, "read_id", &(), LIMIT)?
        .data;
    assert_eq!(owner, EXPECTED_OWNER);
    assert_eq!(self_id, id);

    Ok(())
}
