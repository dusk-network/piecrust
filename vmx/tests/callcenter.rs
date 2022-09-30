// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use uplink::{ModuleId, RawQuery, RawResult, RawTransaction};
use vmx::{module_bytecode, Error, VM};

#[ignore]
pub fn cc_read_counter() -> Result<(), Error> {
    let mut world = VM::ephemeral()?;

    let counter_id = world.deploy(module_bytecode!("counter"))?;

    // read direct

    let value: i64 = world.query(counter_id, "read_value", ())?;
    assert_eq!(value, 0xfc);

    let center_id = world.deploy(module_bytecode!("callcenter"))?;

    // read value through callcenter
    let value: i64 = world.query(center_id, "query_counter", counter_id)?;
    assert_eq!(value, 0xfc);

    Ok(())
}

#[ignore]
pub fn cc_direct() -> Result<(), Error> {
    let mut world = VM::ephemeral()?;

    let counter_id = world.deploy(module_bytecode!("counter"))?;

    // read value directly
    let value: i64 = world.query(counter_id, "read_value", ())?;
    assert_eq!(value, 0xfc);

    let center_id = world.deploy(module_bytecode!("callcenter"))?;

    // read value through callcenter
    let value: i64 = world.query(center_id, "query_counter", counter_id)?;
    assert_eq!(value, 0xfc);

    let mut session = world.session();

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

#[ignore]
pub fn cc_passthrough() -> Result<(), Error> {
    let mut world = VM::ephemeral()?;

    let center_id = world.deploy(module_bytecode!("callcenter"))?;

    let rq = RawQuery::new("read_value", ());

    let res: RawQuery =
        world.query(center_id, "query_passthrough", rq.clone())?;

    assert_eq!(rq, res);

    Ok(())
}

#[ignore]
pub fn cc_delegated_read() -> Result<(), Error> {
    let mut world = VM::ephemeral()?;

    let counter_id = world.deploy(module_bytecode!("counter"))?;
    let center_id = world.deploy(module_bytecode!("callcenter"))?;

    let rq = RawQuery::new("read_value", ());

    // read value through callcenter
    let res = world.query::<_, RawResult>(
        center_id,
        "delegate_query",
        (counter_id, rq),
    )?;

    let value: i64 = res.cast();

    assert_eq!(value, 0xfc);

    Ok(())
}

#[ignore]
pub fn cc_delegated_write() -> Result<(), Error> {
    let mut world = VM::ephemeral()?;

    let counter_id = world.deploy(module_bytecode!("counter"))?;
    let center_id = world.deploy(module_bytecode!("callcenter"))?;

    // increment through delegated transaction

    let rt = RawTransaction::new("increment", ());

    let mut session = world.session();

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

#[ignore]
pub fn cc_self() -> Result<(), Error> {
    let mut world = VM::ephemeral()?;

    let center_id = world.deploy(module_bytecode!("callcenter"))?;

    // am i calling myself
    let calling_self: bool =
        world.query(center_id, "calling_self", center_id)?;
    assert!(calling_self);

    Ok(())
}

#[ignore]
pub fn cc_caller() -> Result<(), Error> {
    let mut world = VM::ephemeral()?;

    let center_id = world.deploy(module_bytecode!("callcenter"))?;

    let value: bool = world.query(center_id, "call_self", ())?;
    assert!(value);

    Ok(())
}

#[ignore]
pub fn cc_caller_uninit() -> Result<(), Error> {
    let mut world = VM::ephemeral()?;

    let center_id = world.deploy(module_bytecode!("callcenter"))?;

    let caller: ModuleId = world.query(center_id, "return_caller", ())?;
    assert_eq!(caller, ModuleId::uninitialized());

    Ok(())
}

#[ignore]
pub fn cc_self_id() -> Result<(), Error> {
    let mut world = VM::ephemeral()?;

    let center_id = world.deploy(module_bytecode!("callcenter"))?;

    let value: ModuleId = world.query(center_id, "return_self_id", ())?;
    assert_eq!(value, center_id);

    Ok(())
}
