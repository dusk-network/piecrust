// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{contract_bytecode, ContractData, Error, SessionData, VM};

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;

#[test]
pub fn debug() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let id = session.deploy(
        contract_bytecode!("debugger"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    session.call::<_, ()>(id, "debug", &String::from("Hello world"), LIMIT)?;

    session.with_debug(|dbg| {
        assert_eq!(dbg, &[String::from("What a string! Hello world")])
    });

    Ok(())
}
