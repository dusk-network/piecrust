// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use dallo::ModuleId;
use hatchery::{module_bytecode, Error, World};
use std::path::PathBuf;

#[test]
pub fn box_set_get() -> Result<(), Error> {
    let mut world = World::ephemeral()?;

    let id = world.deploy(module_bytecode!("box"))?;

    let value: Option<i32> = world.query(id, "get", ())?;

    assert_eq!(value, None);

    world.transact(id, "set", 0x11)?;

    let value: Option<i16> = world.query(id, "get", ())?;

    assert_eq!(value, Some(0x11));

    Ok(())
}

#[test]
pub fn box_set_store_restore_get() -> Result<(), Error> {
    let mut storage_path = PathBuf::new();
    let first_id: ModuleId;

    {
        let mut first_world = World::ephemeral()?;

        first_id = first_world.deploy(module_bytecode!("box"))?;

        first_world.transact(first_id, "set", 0x23)?;

        first_world.storage_path().clone_into(&mut storage_path);
    }

    let mut second_world = World::new(storage_path);

    let second_id = second_world.deploy(module_bytecode!("box"))?;

    assert_eq!(first_id, second_id);

    let value: Option<i16> = second_world.query(second_id, "get", ())?;

    assert_eq!(value, Some(0x23));

    Ok(())
}

#[test]
pub fn box_create_and_restore_snapshots() -> Result<(), Error> {
    const SNAPSHOT1_VALUE: i16 = 17;
    const SNAPSHOT2_COMPRESSED_VALUE: i16 = 100;
    const TRANSACT_VALUE: i16 = 16;
    let mut world = World::ephemeral()?;

    let id = world.deploy(module_bytecode!("box"))?;
    let value: Option<i16> = world.query(id, "get", ())?;
    assert_eq!(value, None);

    world.transact(id, "set", SNAPSHOT1_VALUE)?;
    let snapshot1 = world.uncompressed_snapshot(id)?;
    let value: Option<i16> = world.query(id, "get", ())?;
    assert_eq!(value, Some(SNAPSHOT1_VALUE));
    world.transact(id, "set", SNAPSHOT2_COMPRESSED_VALUE)?;
    let value: Option<i16> = world.query(id, "get", ())?;
    assert_eq!(value, Some(SNAPSHOT2_COMPRESSED_VALUE));
    let snapshot2_compressed = world.compressed_snapshot(id, &snapshot1)?;
    let value: Option<i16> = world.query(id, "get", ())?;
    assert_eq!(value, Some(SNAPSHOT2_COMPRESSED_VALUE));

    world.transact(id, "set", TRANSACT_VALUE)?;
    let value: Option<i16> = world.query(id, "get", ())?;
    assert_eq!(value, Some(TRANSACT_VALUE));

    let id = world.deploy_from_compressed_snapshot(
        module_bytecode!("box"),
        snapshot1.id(),
        snapshot2_compressed.id(),
    )?;
    let value: Option<i16> = world.query(id, "get", ())?;
    assert_eq!(value, Some(SNAPSHOT2_COMPRESSED_VALUE));

    let id = world.deploy_from_uncompressed_snapshot(
        module_bytecode!("box"),
        snapshot1.id(),
    )?;
    let value: Option<i16> = world.query(id, "get", ())?;
    assert_eq!(value, Some(SNAPSHOT1_VALUE));

    Ok(())
}
