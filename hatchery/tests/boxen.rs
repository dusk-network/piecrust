// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use dallo::ModuleId;
use hatchery::{create_snapshot_id, module_bytecode, Error, World};
use std::path::PathBuf;

#[ignore]
pub fn box_set_get() -> Result<(), Error> {
    let mut world = World::ephemeral()?;

    let id = world.deploy(module_bytecode!("box"), 0)?;

    let value: Option<i32> = world.query(id, "get", ())?;

    assert_eq!(value, None);

    world.transact(id, "set", 0x11)?;

    let value: Option<i16> = world.query(id, "get", ())?;

    assert_eq!(value, Some(0x11));

    Ok(())
}

#[ignore]
pub fn box_set_store_restore_get() -> Result<(), Error> {
    let mut storage_path = PathBuf::new();
    let first_id: ModuleId;

    {
        let mut first_world = World::ephemeral()?;

        first_id = first_world.deploy(module_bytecode!("box"), 0)?;

        first_world.transact(first_id, "set", 0x23)?;

        first_world.storage_path().clone_into(&mut storage_path);
    }

    let mut second_world = World::new(storage_path);

    let second_id = second_world.deploy(module_bytecode!("box"), 0)?;

    assert_eq!(first_id, second_id);

    let value: Option<i16> = second_world.query(second_id, "get", ())?;

    assert_eq!(value, Some(0x23));

    Ok(())
}

#[test]
pub fn box_create_and_restore_snapshots() -> Result<(), Error> {
    let mut world = World::ephemeral()?;

    let id = world.deploy(module_bytecode!("box"), 0)?;

    let value: Option<i32> = world.query(id, "get", ())?;

    assert_eq!(value, None);

    println!("setting to 0x11, storing snapshot1");

    world.transact(id, "set", 0x11)?;
    world.create_snapshot(id, create_snapshot_id("snapshot1"))?;

    println!("setting to 0x12, storing snapshot2");

    world.transact(id, "set", 0x13)?;
    world.create_snapshot(id, create_snapshot_id("snapshot2"))?;
    world.create_compressed_snapshot(
        id,
        create_snapshot_id("snapshot1"),
        create_snapshot_id("snapshot3_compressed"),
    )?;

    let value: Option<i16> = world.query(id, "get", ())?;

    println!("confirming get as 0x13");

    assert_eq!(value, Some(0x13));

    println!("restoring snapshot1");

    world.restore_from_snapshot(
        module_bytecode!("box"),
        0,
        create_snapshot_id("snapshot1"),
    )?;

    let value: Option<i16> = world.query(id, "get", ())?;

    println!("confirming get as 0x11");
    assert_eq!(value, Some(0x11));

    println!("restoring snapshot2");

    world.restore_from_snapshot(
        module_bytecode!("box"),
        0,
        create_snapshot_id("snapshot2"),
    )?;

    let value: Option<i16> = world.query(id, "get", ())?;

    println!("confirming get as 0x12");
    assert_eq!(value, Some(0x13));

    world.transact(id, "set", 0x10)?;
    let value: Option<i16> = world.query(id, "get", ())?;
    assert_eq!(value, Some(0x10));

    world.restore_from_compressed_snapshot(
        module_bytecode!("box"),
        0,
        create_snapshot_id("snapshot1"), // base 0x11
        create_snapshot_id("snapshot3_compressed"), // compressed 0x13
    )?;

    let value: Option<i16> = world.query(id, "get", ())?;
    assert_eq!(value, Some(0x13)); // this was recreated from a compressed snapshot taking just 75 bytes,
                                   // diff'ed against `snapshot1`

    Ok(())
}
