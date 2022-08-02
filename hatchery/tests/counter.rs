// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use hatchery::{module_bytecode, Error, World};

#[test]
pub fn counter_trivial() -> Result<(), Error> {
    let mut world = World::ephemeral()?;

    let id = world.deploy(module_bytecode!("counter"), 0)?;

    let value: i32 = world.query(id, "read_value", ())?;

    assert_eq!(value, 0xfc);

    Ok(())
}

#[test]
pub fn counter_increment() -> Result<(), Error> {
    let mut world = World::ephemeral()?;

    let id = world.deploy(module_bytecode!("counter"), 0)?;

    world.transact(id, "increment", ())?;

    let value: i32 = world.query(id, "read_value", ())?;
    assert_eq!(value, 0xfd);

    world.transact(id, "increment", ())?;

    let value: i32 = world.query(id, "read_value", ())?;
    assert_eq!(value, 0xfe);

    Ok(())
}

#[test]
pub fn counter_mogrify() -> Result<(), Error> {
    let mut world = World::ephemeral()?;

    let id = world.deploy(module_bytecode!("counter"), 0)?;

    let value: i32 = world.transact(id, "mogrify", 32)?;

    assert_eq!(value, 0xfc);

    let value: i32 = world.query(id, "read_value", ())?;
    assert_eq!(value, 0xfc - 32);

    Ok(())
}
