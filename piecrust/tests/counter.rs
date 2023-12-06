// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{contract_bytecode, ContractData, Error, SessionData, VM};

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;

#[test]
fn counter_read_simple() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let id = session.deploy(
        contract_bytecode!("counter"),
        ContractData::builder(OWNER),
        LIMIT,
    )?;

    assert_eq!(
        session.call::<_, i64>(id, "read_value", &(), LIMIT)?.data,
        0xfc
    );

    Ok(())
}

#[test]
fn counter_read_write_simple() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let id = session.deploy(
        contract_bytecode!("counter"),
        ContractData::builder(OWNER),
        LIMIT,
    )?;

    assert_eq!(
        session.call::<_, i64>(id, "read_value", &(), LIMIT)?.data,
        0xfc
    );

    session.call::<_, ()>(id, "increment", &(), LIMIT)?;

    assert_eq!(
        session.call::<_, i64>(id, "read_value", &(), LIMIT)?.data,
        0xfd
    );

    Ok(())
}

#[test]
fn call_through_c() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let counter_id = session.deploy(
        contract_bytecode!("counter"),
        ContractData::builder(OWNER),
        LIMIT,
    )?;
    let c_example_id = session.deploy(
        contract_bytecode!("c_example"),
        ContractData::builder(OWNER),
        LIMIT,
    )?;

    assert_eq!(
        session
            .call::<_, i64>(
                c_example_id,
                "increment_and_read",
                &counter_id,
                LIMIT
            )?
            .data,
        0xfd
    );

    Ok(())
}

#[test]
fn increment_panic() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let counter_id = session.deploy(
        contract_bytecode!("fallible_counter"),
        ContractData::builder(OWNER),
        LIMIT,
    )?;

    match session.call::<_, ()>(counter_id, "increment", &true, LIMIT) {
        Err(Error::ContractPanic(panic_msg)) => {
            assert_eq!(panic_msg, String::from("Incremental panic"));
        }
        _ => panic!("Expected a panic error"),
    }

    Ok(())
}
