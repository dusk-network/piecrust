// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{contract_bytecode, ContractData, Error, SessionData, VM};

const OWNER: [u8; 32] = [0u8; 32];

#[test]
fn counter_read_simple() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let id = session
        .deploy(contract_bytecode!("counter"), ContractData::builder(OWNER))?;

    assert_eq!(session.call::<(), i64>(id, "read_value", &())?, 0xfc);

    Ok(())
}

#[test]
fn counter_read_write_simple() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let id = session
        .deploy(contract_bytecode!("counter"), ContractData::builder(OWNER))?;

    assert_eq!(session.call::<(), i64>(id, "read_value", &())?, 0xfc);

    session.call::<(), ()>(id, "increment", &())?;

    assert_eq!(session.call::<(), i64>(id, "read_value", &())?, 0xfd);

    Ok(())
}
