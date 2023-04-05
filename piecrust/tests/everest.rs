// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{module_bytecode, CallData, Error, ModuleData, SessionData, VM};

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 65_536;

#[test]
pub fn height() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    const HEIGHT: u64 = 384u64;
    let mut session =
        vm.genesis_session(SessionData::new().insert("height", HEIGHT));

    let id = session.deploy(
        module_bytecode!("everest"),
        ModuleData::builder(OWNER),
        &CallData::build(LIMIT),
    )?;

    let height: Option<u64> =
        session.transact(id, "get_height", &(), &CallData::build(LIMIT))?;
    assert_eq!(height.unwrap(), HEIGHT);

    Ok(())
}

#[test]
pub fn meta_data_optionality() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.genesis_session(SessionData::new());
    let id = session.deploy(
        module_bytecode!("everest"),
        ModuleData::builder(OWNER),
        &CallData::build(LIMIT),
    )?;
    let height: Option<u64> =
        session.transact(id, "get_height", &(), &CallData::build(LIMIT))?;
    assert!(height.is_none());
    Ok(())
}
