// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{contract_bytecode, ContractData, Error, SessionData, VM};

const CONTRACT_INIT_METHOD: &str = "init";
const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;

#[test]
fn init() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let id = session.deploy(
        contract_bytecode!("initializer"),
        ContractData::builder().owner(OWNER).init_arg(&0xabu8),
        LIMIT,
    )?;

    assert_eq!(
        session.call::<_, u8>(id, "read_value", &(), LIMIT)?.data,
        0xab
    );

    // perform transaction and make sure that the contract works as expected
    session.call::<_, ()>(id, "increment", &(), LIMIT)?;
    assert_eq!(
        session.call::<_, u8>(id, "read_value", &(), LIMIT)?.data,
        0xac
    );

    // we should not be able to call init directly
    let result = session.call::<u8, ()>(id, CONTRACT_INIT_METHOD, &0xaa, LIMIT);
    assert!(
        result.is_err(),
        "calling init directly as transaction should not be allowed"
    );

    // make sure the state is still ok
    assert_eq!(
        session.call::<_, u8>(id, "read_value", &(), LIMIT)?.data,
        0xac
    );

    // initialized state should live through across session boundaries
    let commit_id = session.commit()?;
    let mut session = vm.session(SessionData::builder().base(commit_id))?;
    assert_eq!(
        session.call::<_, u8>(id, "read_value", &(), LIMIT)?.data,
        0xac
    );

    // not being able to call init directly should also be enforced across
    // session boundaries
    let result = session.call::<u8, ()>(id, CONTRACT_INIT_METHOD, &0xae, LIMIT);
    assert!(
        result.is_err(),
        "calling init directly should never be allowed"
    );

    Ok(())
}

#[test]
fn init_indirect_call_blocked() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let empty_initializer_contract_id = session.deploy(
        contract_bytecode!("empty_initializer"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let callcenter_contract_id = session.deploy(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let result = session.call::<_, ()>(
        callcenter_contract_id,
        "call_init",
        &empty_initializer_contract_id,
        LIMIT,
    );

    assert!(
        result.is_err(),
        "calling init indirectly should not be allowed"
    );

    Ok(())
}

#[test]
fn empty_init_argument() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let id = session.deploy(
        contract_bytecode!("empty_initializer"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    assert_eq!(
        session.call::<_, u8>(id, "read_value", &(), LIMIT)?.data,
        0x10
    );

    Ok(())
}
