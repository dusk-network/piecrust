// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{contract_bytecode, ContractData, Error, SessionData, VM};

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;

#[test]
pub fn height() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    const HEIGHT: u64 = 29_000u64;
    let mut session =
        vm.session(SessionData::builder().insert("height", HEIGHT))?;

    let id = session.deploy(
        contract_bytecode!("everest"),
        ContractData::builder(OWNER),
        LIMIT,
    )?;

    let height: Option<u64> = session.call(id, "get_height", &(), LIMIT)?.data;
    assert_eq!(height.unwrap(), HEIGHT);

    Ok(())
}

#[test]
pub fn meta_data_optionality() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;
    let id = session.deploy(
        contract_bytecode!("everest"),
        ContractData::builder(OWNER),
        LIMIT,
    )?;
    let height: Option<u64> = session.call(id, "get_height", &(), LIMIT)?.data;
    assert!(height.is_none());
    Ok(())
}
