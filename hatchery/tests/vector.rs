// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use hatchery::{module_bytecode, Error, Receipt, World};

#[test]
pub fn vector_push_pop() -> Result<(), Error> {
    let mut world = World::ephemeral()?;

    let id = world.deploy(module_bytecode!("vector"))?;

    const N: usize = 128;

    for i in 0..N {
        let _: Receipt<()> = world.transact(0, id, "push", i)?;
    }

    for i in 0..N {
        let popped: Receipt<Option<i16>> = world.transact(0, id, "pop", ())?;

        assert_eq!(*popped, Some((N - i - 1) as i16));
    }

    let popped: Receipt<Option<i16>> = world.transact(0, id, "pop", ())?;

    assert_eq!(*popped, None);

    Ok(())
}
