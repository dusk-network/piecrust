// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use hatchery::{module_bytecode, Error, Receipt, World};

#[test]
pub fn height() -> Result<(), Error> {
    let mut world = World::ephemeral()?;

    let id = world.deploy(module_bytecode!("everest"))?;

    for h in 0..1024 {
        world.set_height(h);
        let height: Receipt<u64> = world.transact(id, "get_height", ())?;
        assert_eq!(*height, h);
    }

    Ok(())
}
