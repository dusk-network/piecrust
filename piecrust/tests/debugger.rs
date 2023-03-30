// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{deploy_data, module_bytecode, DeployData, Error, VM};

const OWNER: [u8; 32] = [0u8; 32];

#[test]
pub fn debug() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.genesis_session();

    let id =
        session.deploy(module_bytecode!("debugger"), deploy_data!(OWNER))?;

    session.query(id, "debug", &String::from("Hello world"))?;

    session.with_debug(|dbg| {
        assert_eq!(dbg, &[String::from("What a string! Hello world")])
    });

    Ok(())
}
