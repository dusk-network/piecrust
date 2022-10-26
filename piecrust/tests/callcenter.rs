// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{module_bytecode, Error, VM};
use piecrust_uplink::{ModuleId, RawQuery, RawResult, RawTransaction};

#[test]
pub fn cc_read_counter() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;

    let mut session = vm.session();

    let counter_id = session.deploy(module_bytecode!("counter"))?;

    // read direct

    let value: i64 = session.query(counter_id, "read_value", ())?;
    assert_eq!(value, 0xfc);

    let center_id = session.deploy(module_bytecode!("callcenter"))?;

    // read value through callcenter
    let value: i64 = session.query(center_id, "query_counter", counter_id)?;
    assert_eq!(value, 0xfc);

    Ok(())
}

#[test]
pub fn cc_direct() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;

    let mut session = vm.session();

    let counter_id = session.deploy(module_bytecode!("counter"))?;

    // read value directly
    let value: i64 = session.query(counter_id, "read_value", ())?;
    assert_eq!(value, 0xfc);

    let center_id = session.deploy(module_bytecode!("callcenter"))?;

    // read value through callcenter
    let value: i64 = session.query(center_id, "query_counter", counter_id)?;
    assert_eq!(value, 0xfc);

    // increment through call center
    session.transact(center_id, "increment_counter", counter_id)?;

    // read value directly
    let value: i64 = session.query(counter_id, "read_value", ())?;
    assert_eq!(value, 0xfd);

    // read value through callcenter
    let value: i64 = session.query(center_id, "query_counter", counter_id)?;
    assert_eq!(value, 0xfd);

    Ok(())
}

#[test]
pub fn cc_passthrough() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;

    let mut session = vm.session();

    let center_id = session.deploy(module_bytecode!("callcenter"))?;

    let rq = RawQuery::new("read_value", ());

    let res: RawQuery =
        session.query(center_id, "query_passthrough", rq.clone())?;

    assert_eq!(rq, res);

    Ok(())
}

#[test]
pub fn cc_delegated_read() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;

    let mut session = vm.session();

    let counter_id = session.deploy(module_bytecode!("counter"))?;
    let center_id = session.deploy(module_bytecode!("callcenter"))?;

    let rq = RawQuery::new("read_value", ());

    // read value through callcenter
    let res = session.query::<_, RawResult>(
        center_id,
        "delegate_query",
        (counter_id, rq),
    )?;

    let value: i64 = res.cast();

    assert_eq!(value, 0xfc);

    Ok(())
}

#[test]
pub fn cc_delegated_write() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;

    // increment through delegated transaction

    let rt = RawTransaction::new("increment", ());

    let mut session = vm.session();
    let counter_id = session.deploy(module_bytecode!("counter"))?;
    let center_id = session.deploy(module_bytecode!("callcenter"))?;

    let _: () = session.transact(
        center_id,
        "delegate_transaction",
        (counter_id, rt),
    )?;

    // read value directly
    let value: i64 = session.query(counter_id, "read_value", ())?;
    assert_eq!(value, 0xfd);

    Ok(())
}

#[test]
pub fn cc_self() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;

    let mut session = vm.session();

    let center_id = session.deploy(module_bytecode!("callcenter"))?;

    // am i calling myself
    let calling_self: bool =
        session.query(center_id, "calling_self", center_id)?;
    assert!(calling_self);

    Ok(())
}

#[test]
pub fn cc_caller() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;

    let mut session = vm.session();

    let center_id = session.deploy(module_bytecode!("callcenter"))?;

    let value: bool = session.query(center_id, "call_self", ())?;
    assert!(value);

    Ok(())
}

#[test]
pub fn cc_caller_uninit() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;

    let mut session = vm.session();

    let center_id = session.deploy(module_bytecode!("callcenter"))?;

    let caller: ModuleId = session.query(center_id, "return_caller", ())?;
    assert_eq!(caller, ModuleId::uninitialized());

    Ok(())
}

#[test]
pub fn cc_self_id() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;

    let mut session = vm.session();

    let center_id = session.deploy(module_bytecode!("callcenter"))?;

    let value: ModuleId = session.query(center_id, "return_self_id", ())?;
    assert_eq!(value, center_id);

    Ok(())
}
