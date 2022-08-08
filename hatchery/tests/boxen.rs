// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use dallo::ModuleId;
use hatchery::{module_bytecode, Error, Receipt, World};
use std::path::PathBuf;

#[test]
pub fn box_set_get() -> Result<(), Error> {
    let mut world = World::ephemeral()?;

    let id = world.deploy(module_bytecode!("box"))?;

    let value: Receipt<Option<i32>> = world.query(0, id, "get", ())?;

    assert_eq!(*value, None);

    let _: Receipt<()> = world.transact(0, id, "set", 0x11)?;

    let value: Receipt<Option<i16>> = world.query(0, id, "get", ())?;

    assert_eq!(*value, Some(0x11));

    Ok(())
}

#[test]
pub fn box_set_store_restore_get() -> Result<(), Error> {
    let mut storage_path = PathBuf::new();
    let first_id: ModuleId;

    {
        let mut first_world = World::ephemeral()?;

        first_id = first_world.deploy(module_bytecode!("box"))?;

        let _: Receipt<()> = first_world.transact(0, first_id, "set", 0x23)?;

        first_world.storage_path().clone_into(&mut storage_path);
    }

    let mut second_world = World::new(storage_path);

    let second_id = second_world.deploy(module_bytecode!("box"))?;

    assert_eq!(first_id, second_id);

    let value: Receipt<Option<i16>> =
        second_world.query(0, second_id, "get", ())?;

    assert_eq!(*value, Some(0x23));

    Ok(())
}

#[test]
pub fn world_persist_restore() -> Result<(), Error> {
    let mut world = World::ephemeral()?;
    let id = world.deploy(module_bytecode!("box"))?;

    let _: Receipt<()> = world.transact(0, id, "set", 17)?;
    let value: Receipt<Option<i16>> = world.query(0, id, "get", ())?;
    assert_eq!(*value, Some(17));

    world.persist()?;

    let _: Receipt<()> = world.transact(0, id, "set", 18)?;
    let value: Receipt<Option<i16>> = world.query(0, id, "get", ())?;
    assert_eq!(*value, Some(18));

    world.restore()?;
    let value: Receipt<Option<i16>> = world.query(0, id, "get", ())?;
    assert_eq!(*value, Some(17));

    Ok(())
}
