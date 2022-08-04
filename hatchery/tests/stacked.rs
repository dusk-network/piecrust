// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use dallo::ModuleId;
use hatchery::{module_bytecode, Error, World};
use std::path::PathBuf;

#[test]
pub fn push_pop() -> Result<(), Error> {
    let mut world = World::ephemeral()?;

    let id = world.deploy(module_bytecode!("stack"))?;

    let val = 42;

    world.transact(id, "push", val)?;

    let len: i32 = world.query(id, "len", ())?;
    assert_eq!(len, 1);

    let popped: Option<i32> = world.transact(id, "pop", ())?;
    let len: i32 = world.query(id, "len", ())?;

    assert_eq!(len, 0);
    assert_eq!(popped, Some(val));

    Ok(())
}

#[test]
pub fn multi_push_pop() -> Result<(), Error> {
    let mut world = World::ephemeral()?;

    let id = world.deploy(module_bytecode!("stack"))?;

    const N: i32 = 1_000;

    for i in 0..N {
        world.transact(id, "push", i)?;
        let len: i32 = world.query(id, "len", ())?;

        assert_eq!(len, i + 1);
    }

    for i in (0..N).rev() {
        let popped: Option<i32> = world.transact(id, "pop", ())?;
        let len: i32 = world.query(id, "len", ())?;

        assert_eq!(len, i);
        assert_eq!(popped, Some(i));
    }

    let popped: Option<i32> = world.transact(id, "pop", ())?;
    assert_eq!(popped, None);

    Ok(())
}
#[test]
pub fn multi_push_store_restore_pop() -> Result<(), Error> {
    let mut storage_path = PathBuf::new();
    let first_id: ModuleId;
    const N: i32 = 1_000;
    {
        let mut first_world = World::ephemeral()?;
        first_id =
            first_world.deploy(module_bytecode!("stack"))?;
        for i in 0..N {
            first_world.transact(first_id, "push", i)?;
            let len: i32 = first_world.query(first_id, "len", ())?;
            assert_eq!(len, i + 1);
        }
        first_world.storage_path().clone_into(&mut storage_path);
    }
    let mut second_world = World::new(storage_path);
    let second_id =
        second_world.deploy(module_bytecode!("stack"))?;
    assert_eq!(first_id, second_id);
    for i in (0..N).rev() {
        let popped: Option<i32> =
            second_world.transact(second_id, "pop", ())?;
        let len: i32 = second_world.query(second_id, "len", ())?;
        assert_eq!(len, i);
        assert_eq!(popped, Some(i));
    }
    let popped: Option<i32> = second_world.transact(second_id, "pop", ())?;
    assert_eq!(popped, None);
    Ok(())
}
