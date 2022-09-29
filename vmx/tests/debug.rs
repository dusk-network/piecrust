// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use vmx::{module_bytecode, Error, VM};

#[test]
pub fn debug() -> Result<(), Error> {
    let mut world = VM::ephemeral()?;

    let id = world.deploy(module_bytecode!("debugger"))?;

    let session = world.session();

    session.query(id, "debug", String::from("Hello world"))?;

    session.with_debug(|dbg| {
        assert_eq!(dbg, &[String::from("What a string! Hello world")])
    });

    Ok(())
}
