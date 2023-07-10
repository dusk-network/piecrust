// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{contract_bytecode, ContractData, Error, SessionData, VM};

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;

#[test]
pub fn fibo() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let id = session.deploy(
        contract_bytecode!("fibonacci"),
        ContractData::builder(OWNER),
        LIMIT,
    )?;

    assert_eq!(session.call::<u32, u64>(id, "nth", &0, LIMIT)?.data, 1);
    assert_eq!(session.call::<u32, u64>(id, "nth", &1, LIMIT)?.data, 1);
    assert_eq!(session.call::<u32, u64>(id, "nth", &2, LIMIT)?.data, 2);
    assert_eq!(session.call::<u32, u64>(id, "nth", &3, LIMIT)?.data, 3);
    assert_eq!(session.call::<u32, u64>(id, "nth", &4, LIMIT)?.data, 5);

    Ok(())
}
