// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use hatchery::{module_bytecode, Error, World};

#[test]
#[ignore]
fn self_snapshot() -> Result<(), Error> {
    let mut world = World::ephemeral()?;

    let id = world.deploy(module_bytecode!("self_snapshot"))?;

    assert_eq!(*world.query::<_, i32>(id, "crossover", ())?, 7);

    // returns old value
    assert_eq!(*world.transact::<_, i32>(id, "set_crossover", 9)?, 7);

    assert_eq!(*world.query::<_, i32>(id, "crossover", ())?, 9);

    world.transact::<_, i32>(id, "self_call_test_a", 10)?;

    assert_eq!(*world.query::<_, i32>(id, "crossover", ())?, 10);

    let result = world.transact::<_, i32>(id, "update_and_panic", 11);

    assert!(result.is_err());

    // panic reverted the change!

    assert_eq!(*world.query::<_, i32>(id, "crossover", ())?, 10);

    Ok(())
}
