// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{contract_bytecode, ContractData, Error, SessionData, VM};
use piecrust_uplink::{ContractError, ContractId};

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;

#[tokio::test(flavor = "multi_thread")]
pub async fn cc_read_counter() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let counter_id = session.deploy(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    // read direct

    let value: i64 = session.call(counter_id, "read_value", &(), LIMIT)?.data;
    assert_eq!(value, 0xfc);

    let center_id = session.deploy(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    // read value through callcenter
    let value: i64 = session
        .call(center_id, "query_counter", &counter_id, LIMIT)?
        .data;
    assert_eq!(value, 0xfc);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
pub async fn cc_direct() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let counter_id = session.deploy(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    // read value directly
    let value: i64 = session.call(counter_id, "read_value", &(), LIMIT)?.data;
    assert_eq!(value, 0xfc);

    let center_id = session.deploy(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    // read value through callcenter
    let value: i64 = session
        .call(center_id, "query_counter", &counter_id, LIMIT)?
        .data;
    assert_eq!(value, 0xfc);

    // increment through call center
    session.call::<_, ()>(
        center_id,
        "increment_counter",
        &counter_id,
        LIMIT,
    )?;

    // read value directly
    let value: i64 = session.call(counter_id, "read_value", &(), LIMIT)?.data;
    assert_eq!(value, 0xfd);

    // read value through callcenter
    let value: i64 = session
        .call(center_id, "query_counter", &counter_id, LIMIT)?
        .data;
    assert_eq!(value, 0xfd);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
pub async fn cc_passthrough() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let center_id = session.deploy(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let raw = (String::from("read_value"), Vec::<u8>::new());

    let res: (String, Vec<u8>) = session
        .call(center_id, "query_passthrough", &raw, LIMIT)?
        .data;

    assert_eq!(raw, res);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
pub async fn cc_delegated_read() -> Result<(), Error> {
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

    // read value through callcenter
    let res = session
        .call::<_, Result<Vec<u8>, ContractError>>(
            center_id,
            "delegate_query",
            &(counter_id, String::from("read_value"), Vec::<u8>::new()),
            LIMIT,
        )?
        .data
        .expect("ICC should succeed");

    let value: i64 =
        rkyv::from_bytes(&res).expect("Deserialization to succeed");

    assert_eq!(value, 0xfc);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
pub async fn cc_delegated_write() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    // increment through delegated transaction

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

    session.call::<_, ()>(
        center_id,
        "delegate_transaction",
        &(counter_id, String::from("increment"), Vec::<u8>::new()),
        LIMIT,
    )?;

    // read value directly
    let value: i64 = session.call(counter_id, "read_value", &(), LIMIT)?.data;
    assert_eq!(value, 0xfd);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
pub async fn cc_self() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let center_id = session.deploy(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    // am i calling myself
    let calling_self: bool = session
        .call(center_id, "calling_self", &center_id, LIMIT)?
        .data;
    assert!(calling_self);

    Ok(())
}

// #[tokio::test(flavor = "multi_thread")]
#[ignore]
pub async fn cc_caller() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let center_id = session.deploy(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let value: Result<bool, ContractError> =
        session.call(center_id, "call_self", &(), LIMIT)?.data;

    assert!(value.expect("should succeed"));

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
pub async fn cc_caller_uninit() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let center_id = session.deploy(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let caller: Option<ContractId> =
        session.call(center_id, "return_caller", &(), LIMIT)?.data;
    assert_eq!(caller, None);

    Ok(())
}

// #[tokio::test(flavor = "multi_thread")]
#[ignore]
pub async fn cc_callstack() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let center_id = session.deploy(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let callstack_id = session.deploy(
        contract_bytecode!("callstack"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let callstack: Vec<ContractId> = session
        .call(center_id, "return_callstack", &(), LIMIT)?
        .data;
    assert_eq!(callstack.len(), 0);

    let self_id: ContractId =
        session.call(center_id, "return_self_id", &(), LIMIT)?.data;

    const N: u32 = 5;
    let callstack: Vec<ContractId> = session
        .call(center_id, "call_self_n_times", &N, LIMIT)?
        .data;
    assert_eq!(callstack.len(), N as usize);
    for i in 0..N as usize {
        assert_eq!(self_id, callstack[i]);
    }

    let res = session
        .call::<_, Result<Vec<u8>, ContractError>>(
            center_id,
            "delegate_query",
            &(
                callstack_id,
                String::from("return_callstack"),
                Vec::<u8>::new(),
            ),
            LIMIT,
        )?
        .data
        .expect("ICC should succeed");

    let callstack: Vec<ContractId> =
        rkyv::from_bytes(&res).expect("Deserialization to succeed");

    assert_eq!(callstack.len(), 1);
    assert_eq!(callstack[0], center_id);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
pub async fn cc_self_id() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let center_id = session.deploy(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let value: ContractId =
        session.call(center_id, "return_self_id", &(), LIMIT)?.data;
    assert_eq!(value, center_id);

    Ok(())
}
