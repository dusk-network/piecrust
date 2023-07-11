// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{contract_bytecode, ContractData, Error, SessionData, VM};
use piecrust_uplink::ContractId;

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;

#[test]
pub fn deploy_with_id() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let bytecode = contract_bytecode!("counter");
    let some_id = [1u8; 32];
    let contract_id = ContractId::from(some_id);
    let mut session = vm.session(SessionData::builder())?;
    session.deploy(
        bytecode,
        ContractData::builder(OWNER).contract_id(contract_id),
        LIMIT,
    )?;

    assert_eq!(
        session
            .call::<_, i64>(contract_id, "read_value", &(), LIMIT)?
            .data,
        0xfc
    );

    session.call::<_, ()>(contract_id, "increment", &(), LIMIT)?;

    assert_eq!(
        session
            .call::<_, i64>(contract_id, "read_value", &(), LIMIT)?
            .data,
        0xfd
    );

    Ok(())
}
