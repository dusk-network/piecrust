// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{
    contract_bytecode, ContractData, ContractError, ContractId, Error,
    SessionData, VM,
};

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;

#[test]
pub fn deploy_with_id() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let bytecode = contract_bytecode!("counter");
    let some_id = [1u8; 32];
    let contract_id = ContractId::from(some_id);
    let mut session = vm.session(None, SessionData::builder())?;
    session.deploy(Some(contract_id), bytecode, &(), OWNER, LIMIT)?;

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

#[test]
fn call_non_deployed() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let bytecode = contract_bytecode!("double_counter");
    let counter_id = ContractId::from_bytes([1; 32]);
    let mut session = vm.session(None, SessionData::builder())?;
    session.deploy(Some(counter_id), bytecode, &(), OWNER, LIMIT)?;

    let (value, _) = session
        .call::<_, (i64, i64)>(counter_id, "read_values", &(), LIMIT)?
        .data;
    assert_eq!(value, 0xfc);

    let bogus_id = ContractId::from_bytes([255; 32]);
    let r = session
        .call::<_, Result<(), ContractError>>(
            counter_id,
            "increment_left_and_call",
            &bogus_id,
            LIMIT,
        )?
        .data;

    assert!(matches!(r, Err(ContractError::DoesNotExist)));

    let (value, _) = session
        .call::<_, (i64, i64)>(counter_id, "read_values", &(), LIMIT)?
        .data;
    assert_eq!(value, 0xfd);

    Ok(())
}
