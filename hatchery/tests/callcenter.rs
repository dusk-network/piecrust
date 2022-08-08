// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use hatchery::{module_bytecode, Error, Receipt, World};

#[test]
pub fn world_center_counter_read() -> Result<(), Error> {
    let mut world = World::ephemeral()?;

    let counter_id = world.deploy(module_bytecode!("counter"))?;

    let value: Receipt<i64> = world.query(counter_id, "read_value", ())?;
    assert_eq!(*value, 0xfc);

    let center_id = world.deploy(module_bytecode!("callcenter"))?;

    // read value through callcenter
    let value: Receipt<i64> =
        world.query(center_id, "query_counter", counter_id)?;
    assert_eq!(*value, 0xfc);

    Ok(())
}

#[test]
pub fn world_center_counter() -> Result<(), Error> {
    let mut world = World::ephemeral()?;

    let counter_id = world.deploy(module_bytecode!("counter"))?;

    // read value directly
    let value: Receipt<i64> = world.query(counter_id, "read_value", ())?;
    assert_eq!(*value, 0xfc);

    let center_id = world.deploy(module_bytecode!("callcenter"))?;

    // read value through callcenter
    let value: Receipt<i64> =
        world.query(center_id, "query_counter", counter_id)?;
    assert_eq!(*value, 0xfc);

    // increment through call center
    let _: Receipt<()> =
        world.transact(center_id, "increment_counter", counter_id)?;

    // read value directly
    let value: Receipt<i64> = world.query(counter_id, "read_value", ())?;
    assert_eq!(*value, 0xfd);

    // read value through callcenter
    let value: Receipt<i64> =
        world.query(center_id, "query_counter", counter_id)?;
    assert_eq!(*value, 0xfd);

    Ok(())
}

#[test]
pub fn world_center_calls_self() -> Result<(), Error> {
    let mut world = World::ephemeral()?;

    let center_id = world.deploy(module_bytecode!("callcenter"))?;

    // am i calling myself
    let calling_self: Receipt<bool> =
        world.query(center_id, "calling_self", center_id)?;
    assert!(*calling_self);

    Ok(())
}

#[test]
pub fn world_center_caller() -> Result<(), Error> {
    let mut world = World::ephemeral()?;

    let center_id = world.deploy(module_bytecode!("callcenter"))?;

    let value: Receipt<bool> = world.query(center_id, "call_self", ())?;
    assert!(*value);

    Ok(())
}
