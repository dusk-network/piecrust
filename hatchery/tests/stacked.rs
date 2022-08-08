// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use hatchery::{module_bytecode, Error, Receipt, World};

#[test]
pub fn push_pop() -> Result<(), Error> {
    let mut world = World::ephemeral()?;

    let id = world.deploy(module_bytecode!("stack"))?;

    let val = 42;

    let _: Receipt<()> = world.transact(0, id, "push", val)?;

    let len: Receipt<i32> = world.query(0, id, "len", ())?;
    assert_eq!(*len, 1);

    let popped: Receipt<Option<i32>> = world.transact(0, id, "pop", ())?;
    let len: Receipt<i32> = world.query(0, id, "len", ())?;

    assert_eq!(*len, 0);
    assert_eq!(*popped, Some(val));

    Ok(())
}

#[test]
pub fn multi_push_pop() -> Result<(), Error> {
    let mut world = World::ephemeral()?;

    let id = world.deploy(module_bytecode!("stack"))?;

    const N: i32 = 1_000;

    for i in 0..N {
        let _: Receipt<()> = world.transact(0, id, "push", i)?;
        let len: Receipt<i32> = world.query(0, id, "len", ())?;

        assert_eq!(*len, i + 1);
    }

    for i in (0..N).rev() {
        let popped: Receipt<Option<i32>> = world.transact(0, id, "pop", ())?;
        let len: Receipt<i32> = world.query(0, id, "len", ())?;

        assert_eq!(*len, i);
        assert_eq!(*popped, Some(i));
    }

    let popped: Receipt<Option<i32>> = world.transact(0, id, "pop", ())?;
    assert_eq!(*popped, None);

    Ok(())
}
