// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{contract_bytecode, ContractData, Error, SessionData, VM};
use piecrust_uplink::{ContractError, ContractId, RawCall, RawResult};

const OWNER: [u8; 32] = [0u8; 32];

#[test]
pub fn cc_read_counter() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let counter_id = session
        .deploy(contract_bytecode!("counter"), ContractData::builder(OWNER))?;

    // read direct

    let value: i64 = session.call(counter_id, "read_value", &())?;
    assert_eq!(value, 0xfc);

    let center_id = session.deploy(
        contract_bytecode!("callcenter"),
        ContractData::builder(OWNER),
    )?;

    // read value through callcenter
    let value: i64 = session.call(center_id, "query_counter", &counter_id)?;
    assert_eq!(value, 0xfc);

    Ok(())
}

#[test]
pub fn cc_direct() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let counter_id = session
        .deploy(contract_bytecode!("counter"), ContractData::builder(OWNER))?;

    // read value directly
    let value: i64 = session.call(counter_id, "read_value", &())?;
    assert_eq!(value, 0xfc);

    let center_id = session.deploy(
        contract_bytecode!("callcenter"),
        ContractData::builder(OWNER),
    )?;

    // read value through callcenter
    let value: i64 = session.call(center_id, "query_counter", &counter_id)?;
    assert_eq!(value, 0xfc);

    // increment through call center
    session.call(center_id, "increment_counter", &counter_id)?;

    // read value directly
    let value: i64 = session.call(counter_id, "read_value", &())?;
    assert_eq!(value, 0xfd);

    // read value through callcenter
    let value: i64 = session.call(center_id, "query_counter", &counter_id)?;
    assert_eq!(value, 0xfd);

    Ok(())
}

#[test]
pub fn cc_passthrough() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let center_id = session.deploy(
        contract_bytecode!("callcenter"),
        ContractData::builder(OWNER),
    )?;

    let rq = RawCall::new("read_value", ());

    let res: RawCall = session.call(center_id, "query_passthrough", &rq)?;

    assert_eq!(rq, res);

    Ok(())
}

#[test]
pub fn cc_delegated_read() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let counter_id = session
        .deploy(contract_bytecode!("counter"), ContractData::builder(OWNER))?;
    let center_id = session.deploy(
        contract_bytecode!("callcenter"),
        ContractData::builder(OWNER),
    )?;

    let rq = RawCall::new("read_value", ());

    // read value through callcenter
    let res = session.call::<_, RawResult>(
        center_id,
        "delegate_query",
        &(counter_id, rq),
    )?;

    let value: i64 = res.cast();

    assert_eq!(value, 0xfc);

    Ok(())
}

#[test]
pub fn cc_delegated_write() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    // increment through delegated transaction

    let rt = RawCall::new("increment", ());

    let mut session = vm.session(SessionData::builder())?;
    let counter_id = session
        .deploy(contract_bytecode!("counter"), ContractData::builder(OWNER))?;
    let center_id = session.deploy(
        contract_bytecode!("callcenter"),
        ContractData::builder(OWNER),
    )?;

    session.call(center_id, "delegate_transaction", &(counter_id, rt))?;

    // read value directly
    let value: i64 = session.call(counter_id, "read_value", &())?;
    assert_eq!(value, 0xfd);

    Ok(())
}

#[test]
pub fn cc_self() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let center_id = session.deploy(
        contract_bytecode!("callcenter"),
        ContractData::builder(OWNER),
    )?;

    // am i calling myself
    let calling_self: bool =
        session.call(center_id, "calling_self", &center_id)?;
    assert!(calling_self);

    Ok(())
}

#[test]
pub fn cc_caller() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let center_id = session.deploy(
        contract_bytecode!("callcenter"),
        ContractData::builder(OWNER),
    )?;

    let value: Result<bool, ContractError> =
        session.call(center_id, "call_self", &())?;

    assert!(value.expect("should succeed"));

    Ok(())
}

#[test]
pub fn cc_caller_uninit() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let center_id = session.deploy(
        contract_bytecode!("callcenter"),
        ContractData::builder(OWNER),
    )?;

    let caller: ContractId = session.call(center_id, "return_caller", &())?;
    assert_eq!(caller, ContractId::uninitialized());

    Ok(())
}

#[test]
pub fn cc_self_id() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let center_id = session.deploy(
        contract_bytecode!("callcenter"),
        ContractData::builder(OWNER),
    )?;

    let value: ContractId = session.call(center_id, "return_self_id", &())?;
    assert_eq!(value, center_id);

    Ok(())
}
