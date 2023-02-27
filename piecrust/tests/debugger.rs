// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{module_bytecode, Error, VM};

#[test]
pub fn debug() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.genesis_session();

    let id = session.deploy(module_bytecode!("debugger"))?;

    session.query(id, "debug", &String::from("Hello world"))?;

    session.with_debug(|dbg| {
        assert_eq!(dbg, &[String::from("What a string! Hello world")])
    });

    Ok(())
}
