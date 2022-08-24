// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use dallo::ModuleId;
use hatchery::{module_bytecode, Error, Receipt, SnapshotId, World};

#[test]
pub fn snapshot_persist_restore() -> Result<(), Error> {
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
        let _: Receipt<()> = world.transact(id, "mem_snap", ())?;
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
pub fn world_revert_reverts_module_snapshot_ids() -> Result<(), Error> {
    let mut world = World::ephemeral()?;
    let id = world.deploy(module_bytecode!("box"))?;

    world.transact::<i16, ()>(id, "set", 0x23)?;
    let value = world.query::<_, Option<i16>>(id, "get", ())?;
    assert_eq!(*value, Some(0x23));

    let snapshot_id1 = world.persist()?;

    world.transact::<i16, ()>(id, "set", 0x24)?;
    let value = world.query::<_, Option<i16>>(id, "get", ())?;
    assert_eq!(*value, Some(0x24));

    world.restore(&snapshot_id1)?;

    let snapshot_id2 = world.persist()?;

    // all module snapshot ids have been reverted
    // otherwise they would not contribute to the same (world) snapshot id
    assert_eq!(snapshot_id1, snapshot_id2);

    Ok(())
}
