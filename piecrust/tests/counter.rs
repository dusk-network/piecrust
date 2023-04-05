// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{module_bytecode, CallData, Error, ModuleData, SessionData, VM};

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 65_536;

#[test]
fn counter_read_simple() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.genesis_session(SessionData::new());

    let id = session.deploy(
        module_bytecode!("counter"),
        ModuleData::builder(OWNER),
        &CallData::build(LIMIT),
    )?;

    assert_eq!(
        session.query::<(), i64>(
            id,
            "read_value",
            &(),
            &CallData::build(LIMIT)
        )?,
        0xfc
    );

    Ok(())
}

#[test]
fn counter_read_write_simple() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.genesis_session(SessionData::new());

    let id = session.deploy(
        module_bytecode!("counter"),
        ModuleData::builder(OWNER),
        &CallData::build(LIMIT),
    )?;

    assert_eq!(
        session.query::<(), i64>(
            id,
            "read_value",
            &(),
            &CallData::build(LIMIT)
        )?,
        0xfc
    );

    session.transact::<(), ()>(
        id,
        "increment",
        &(),
        &CallData::build(LIMIT),
    )?;

    assert_eq!(
        session.query::<(), i64>(
            id,
            "read_value",
            &(),
            &CallData::build(LIMIT)
        )?,
        0xfd
    );

    Ok(())
}
