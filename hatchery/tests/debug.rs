// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use hatchery::{module_bytecode, Error, Receipt, World};

#[test]
pub fn debug() -> Result<(), Error> {
    let mut world = World::ephemeral()?;

    let id = world.deploy(module_bytecode!("debugger"))?;

    let res: Receipt<()> =
        world.query(id, "debug", String::from("Hello world"))?;

    assert_eq!(res.debug(), &[String::from("What a string! Hello world")]);

    Ok(())
}
