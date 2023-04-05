// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{module_bytecode, CallData, Error, ModuleData, SessionData, VM};
use piecrust_uplink::{
    ModuleError, ModuleId, RawQuery, RawResult, RawTransaction,
};

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 65_536;

#[test]
pub fn cc_read_counter() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.genesis_session(SessionData::new());

    let counter_id = session.deploy(
        module_bytecode!("counter"),
        ModuleData::builder(OWNER),
        &CallData::build(LIMIT),
    )?;

    // read direct

    let value: i64 = session.query(
        counter_id,
        "read_value",
        &(),
        &CallData::build(LIMIT),
    )?;
    assert_eq!(value, 0xfc);

    let center_id = session.deploy(
        module_bytecode!("callcenter"),
        ModuleData::builder(OWNER),
        &CallData::build(LIMIT),
    )?;

    // read value through callcenter
    let value: i64 = session.query(
        center_id,
        "query_counter",
        &counter_id,
        &CallData::build(LIMIT),
    )?;
    assert_eq!(value, 0xfc);

    Ok(())
}

#[test]
pub fn cc_direct() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.genesis_session(SessionData::new());

    let counter_id = session.deploy(
        module_bytecode!("counter"),
        ModuleData::builder(OWNER),
        &CallData::build(LIMIT),
    )?;

    // read value directly
    let value: i64 = session.query(
        counter_id,
        "read_value",
        &(),
        &CallData::build(LIMIT),
    )?;
    assert_eq!(value, 0xfc);

    let center_id = session.deploy(
        module_bytecode!("callcenter"),
        ModuleData::builder(OWNER),
        &CallData::build(LIMIT),
    )?;

    // read value through callcenter
    let value: i64 = session.query(
        center_id,
        "query_counter",
        &counter_id,
        &CallData::build(LIMIT),
    )?;
    assert_eq!(value, 0xfc);

    // increment through call center
    session.transact(
        center_id,
        "increment_counter",
        &counter_id,
        &CallData::build(LIMIT),
    )?;

    // read value directly
    let value: i64 = session.query(
        counter_id,
        "read_value",
        &(),
        &CallData::build(LIMIT),
    )?;
    assert_eq!(value, 0xfd);

    // read value through callcenter
    let value: i64 = session.query(
        center_id,
        "query_counter",
        &counter_id,
        &CallData::build(LIMIT),
    )?;
    assert_eq!(value, 0xfd);

    Ok(())
}

#[test]
pub fn cc_passthrough() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.genesis_session(SessionData::new());

    let center_id = session.deploy(
        module_bytecode!("callcenter"),
        ModuleData::builder(OWNER),
        &CallData::build(LIMIT),
    )?;

    let rq = RawQuery::new("read_value", ());

    let res: RawQuery = session.query(
        center_id,
        "query_passthrough",
        &rq,
        &CallData::build(LIMIT),
    )?;

    assert_eq!(rq, res);

    Ok(())
}

#[test]
pub fn cc_delegated_read() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.genesis_session(SessionData::new());

    let counter_id = session.deploy(
        module_bytecode!("counter"),
        ModuleData::builder(OWNER),
        &CallData::build(LIMIT),
    )?;
    let center_id = session.deploy(
        module_bytecode!("callcenter"),
        ModuleData::builder(OWNER),
        &CallData::build(LIMIT),
    )?;

    let rq = RawQuery::new("read_value", ());

    // read value through callcenter
    let res = session.query::<_, RawResult>(
        center_id,
        "delegate_query",
        &(counter_id, rq),
        &CallData::build(LIMIT),
    )?;

    let value: i64 = res.cast();

    assert_eq!(value, 0xfc);

    Ok(())
}

#[test]
pub fn cc_delegated_write() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    // increment through delegated transaction

    let rt = RawTransaction::new("increment", ());

    let mut session = vm.genesis_session(SessionData::new());
    let counter_id = session.deploy(
        module_bytecode!("counter"),
        ModuleData::builder(OWNER),
        &CallData::build(LIMIT),
    )?;
    let center_id = session.deploy(
        module_bytecode!("callcenter"),
        ModuleData::builder(OWNER),
        &CallData::build(LIMIT),
    )?;

    session.transact(
        center_id,
        "delegate_transaction",
        &(counter_id, rt),
        &CallData::build(LIMIT),
    )?;

    // read value directly
    let value: i64 = session.query(
        counter_id,
        "read_value",
        &(),
        &CallData::build(LIMIT),
    )?;
    assert_eq!(value, 0xfd);

    Ok(())
}

#[test]
pub fn cc_self() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.genesis_session(SessionData::new());

    let center_id = session.deploy(
        module_bytecode!("callcenter"),
        ModuleData::builder(OWNER),
        &CallData::build(LIMIT),
    )?;

    // am i calling myself
    let calling_self: bool = session.query(
        center_id,
        "calling_self",
        &center_id,
        &CallData::build(LIMIT),
    )?;
    assert!(calling_self);

    Ok(())
}

#[test]
pub fn cc_caller() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.genesis_session(SessionData::new());

    let center_id = session.deploy(
        module_bytecode!("callcenter"),
        ModuleData::builder(OWNER),
        &CallData::build(LIMIT),
    )?;

    let value: Result<bool, ModuleError> =
        session.query(center_id, "call_self", &(), &CallData::build(LIMIT))?;

    assert!(value.expect("should succeed"));

    Ok(())
}

#[test]
pub fn cc_caller_uninit() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.genesis_session(SessionData::new());

    let center_id = session.deploy(
        module_bytecode!("callcenter"),
        ModuleData::builder(OWNER),
        &CallData::build(LIMIT),
    )?;

    let caller: ModuleId = session.query(
        center_id,
        "return_caller",
        &(),
        &CallData::build(LIMIT),
    )?;
    assert_eq!(caller, ModuleId::uninitialized());

    Ok(())
}

#[test]
pub fn cc_self_id() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.genesis_session(SessionData::new());

    let center_id = session.deploy(
        module_bytecode!("callcenter"),
        ModuleData::builder(OWNER),
        &CallData::build(LIMIT),
    )?;

    let value: ModuleId = session.query(
        center_id,
        "return_self_id",
        &(),
        &CallData::build(LIMIT),
    )?;
    assert_eq!(value, center_id);

    Ok(())
}
