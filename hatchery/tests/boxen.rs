// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use dallo::ModuleId;
use hatchery::Error::{PersistenceError, SnapshotError};
use hatchery::{module_bytecode, Error, Receipt, SnapshotId, World};
use std::path::PathBuf;

#[ignore]
pub fn box_set_get() -> Result<(), Error> {
    let mut world = World::ephemeral()?;

    let id = world.deploy(module_bytecode!("box"))?;

    let value: Receipt<Option<i16>> = world.query(id, "get", ())?;

    assert_eq!(*value, None);

    world.transact::<i16, ()>(id, "set", 0x11)?;

    let value = world.query::<_, Option<i16>>(id, "get", ())?;

    assert_eq!(*value, Some(0x11));

    Ok(())
}

#[ignore]
pub fn box_set_store_restore_get() -> Result<(), Error> {
    let mut storage_path = PathBuf::new();
    let first_id: ModuleId;

    {
        let mut first_world = World::ephemeral()?;

        first_id = first_world.deploy(module_bytecode!("box"))?;

        first_world.transact::<i16, ()>(first_id, "set", 0x23)?;

        first_world.storage_path().clone_into(&mut storage_path);
    }

    let mut second_world = World::new(storage_path);

    let second_id = second_world.deploy(module_bytecode!("box"))?;

    assert_eq!(first_id, second_id);

    let value = second_world.query::<_, Option<i16>>(second_id, "get", ())?;

    assert_eq!(*value, Some(0x23));

    Ok(())
}

#[ignore]
pub fn world_snapshot_persist_restore() -> Result<(), Error> {
    let mut world = World::ephemeral()?;
    let id = world.deploy(module_bytecode!("box"))?;

    fn create_snapshot(
        world: &mut World,
        id: ModuleId,
        arg: i16,
    ) -> Result<SnapshotId, Error> {
        let _: Receipt<()> = world.transact(id, "set", arg)?;

        let value = world.query::<_, Option<i16>>(id, "get", ())?;

        assert_eq!(*value, Some(arg));

        world.persist()
    }

    fn restore_snapshot(
        world: &mut World,
        id: ModuleId,
        snapshot_id: &SnapshotId,
        arg: i16,
    ) -> Result<(), Error> {
        world.restore(&snapshot_id)?;
        let value = world.query::<_, Option<i16>>(id, "get", ())?;
        assert_eq!(*value, Some(arg));
        Ok(())
    }

    let mut snapshot_ids = Vec::new();
    let random_i = vec![3, 1, 0, 4, 2];
    for i in 0..random_i.len() {
        snapshot_ids.push(create_snapshot(&mut world, id, i as i16)?);
    }
    for i in random_i {
        restore_snapshot(&mut world, id, &snapshot_ids[i], (i) as i16)?;
    }
    Ok(())
}

#[test]
pub fn snapshot_hash_excludes_argbuf() -> Result<(), Error> {
    use hatchery::ByteArrayWrapper;
    let mut world = World::ephemeral()?;
    let id = world.deploy(module_bytecode!("box"))?;

    let snapshot_id1 = world.persist()?;
    let _: Receipt<()> = world.transact(id, "noop", 0x22)?;
    let _: Receipt<()> = world.transact(id, "mem_snap", ())?;
    let snapshot_id2 = world.persist()?;
    let _: Receipt<()> = world.transact(id, "noop", 0x22)?;
    let _: Receipt<()> = world.transact(id, "mem_snap", ())?;
    let snapshot_id3 = world.persist()?;
    assert_eq!(snapshot_id2, snapshot_id3);

    println!("snapshot1 = {}", ByteArrayWrapper(snapshot_id1.as_bytes()));
    println!("snapshot2 = {}", ByteArrayWrapper(snapshot_id2.as_bytes()));
    println!("snapshot3 = {}", ByteArrayWrapper(snapshot_id3.as_bytes()));
    // Err(SnapshotError(String::from("abc")))
    Ok(())
}
