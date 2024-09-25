// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{contract_bytecode, ContractData, Error, SessionData, VM};
use piecrust_uplink::ContractError;

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;

#[test]
pub fn gas_get_used() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let counter_id = session.deploy(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    let center_id = session.deploy(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let receipt =
        session.call::<_, i64>(counter_id, "read_value", &(), LIMIT)?;
    let counter_spent = receipt.gas_spent;

    let receipt = session.call::<_, i64>(
        center_id,
        "query_counter",
        &counter_id,
        LIMIT,
    )?;
    let center_spent = receipt.gas_spent;

    assert!(counter_spent < center_spent);

    Ok(())
}

#[ignore]
pub fn panic_msg_gets_through() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let spender_id = session.deploy(
        contract_bytecode!("spender"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    let callcenter_id = session.deploy(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let receipt = session.call::<_, Result<(), ContractError>>(
        callcenter_id,
        "call_spend_with_limit",
        &(spender_id, 4845u64),
        LIMIT,
    )?;

    assert!(
        matches!(receipt.data, Err(ContractError::Panic(x)) if x == "I like spending")
    );

    Ok(())
}

#[test]
pub fn fails_with_out_of_gas() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let counter_id = session.deploy(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let err = session
        .call::<_, i64>(counter_id, "read_value", &(), 1)
        .expect_err("should error with no gas");

    assert!(matches!(err, Error::OutOfGas));

    Ok(())
}

#[test]
pub fn contract_sets_call_limit() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session_1st = vm.session(SessionData::builder())?;
    let mut session_2nd = vm.session(SessionData::builder())?;

    session_1st.deploy(
        contract_bytecode!("spender"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    session_1st.deploy(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let spender_id = session_2nd.deploy(
        contract_bytecode!("spender"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    let callcenter_id = session_2nd.deploy(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    const FIRST_LIMIT: u64 = 1175;
    const SECOND_LIMIT: u64 = 2175;

    let receipt = session_1st.call::<_, Result<(), ContractError>>(
        callcenter_id,
        "call_spend_with_limit",
        &(spender_id, FIRST_LIMIT),
        LIMIT,
    )?;
    let spent_first = receipt.gas_spent;

    let receipt = session_2nd.call::<_, Result<(), ContractError>>(
        callcenter_id,
        "call_spend_with_limit",
        &(spender_id, SECOND_LIMIT),
        LIMIT,
    )?;
    let spent_second = receipt.gas_spent;

    assert_eq!(spent_second - spent_first, SECOND_LIMIT - FIRST_LIMIT);

    Ok(())
}

#[test]
pub fn limit_and_spent() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    const LIMIT: u64 = 10000;

    let mut session = vm.session(SessionData::builder())?;

    let spender_id = session.deploy(
        contract_bytecode!("spender"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let receipt = session.call::<_, (u64, u64, u64, u64, u64)>(
        spender_id,
        "get_limit_and_spent",
        &(),
        LIMIT,
    )?;

    let (limit, spent_before, spent_after, called_limit, called_spent) =
        receipt.data;
    let spender_spent = receipt.gas_spent;

    assert_eq!(limit, LIMIT, "should be the initial limit");

    println!("=== Spender costs ===");

    println!("limit       : {}", limit);
    println!("spent before: {}", spent_before);
    println!("spent after : {}\n", spent_after);
    println!("called limit: {}", called_limit);
    println!("called spent: {}", called_spent);

    println!("===  Actual cost  ===");
    println!("actual cost : {}", spender_spent);

    Ok(())
}
