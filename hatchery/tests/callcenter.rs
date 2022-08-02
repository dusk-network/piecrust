// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use hatchery::{module_bytecode, Error, World};

#[test]
pub fn world_center_counter_read() -> Result<(), Error> {
    let mut world = World::ephemeral()?;

    let counter_id = world.deploy(module_bytecode!("counter"), 0)?;

    let value: i64 = world.query(counter_id, "read_value", ())?;
    assert_eq!(value, 0xfc);

    let center_id = world.deploy(module_bytecode!("callcenter"), 0)?;

    // read value through callcenter
    let value: i64 = world.query(center_id, "query_counter", counter_id)?;
    assert_eq!(value, 0xfc);

    Ok(())
}

#[test]
pub fn world_center_counter() -> Result<(), Error> {
    let mut world = World::ephemeral()?;

    let counter_id = world.deploy(module_bytecode!("counter"), 0)?;

    // read value directly
    let value: i64 = world.query(counter_id, "read_value", ())?;
    assert_eq!(value, 0xfc);

    let center_id = world.deploy(module_bytecode!("callcenter"), 0)?;

    // read value through callcenter
    let value: i64 = world.query(center_id, "query_counter", counter_id)?;
    assert_eq!(value, 0xfc);

    // increment through call center
    world.transact(center_id, "increment_counter", counter_id)?;

    // read value directly
    let value: i64 = world.query(counter_id, "read_value", ())?;
    assert_eq!(value, 0xfd);

    // read value through callcenter
    let value: i64 = world.query(center_id, "query_counter", counter_id)?;
    assert_eq!(value, 0xfd);

    Ok(())
}
