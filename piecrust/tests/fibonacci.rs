// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{contract_bytecode, ContractData, Error, SessionData, VM};

const OWNER: [u8; 32] = [0u8; 32];

#[test]
pub fn fibo() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let id = session.deploy(
        contract_bytecode!("fibonacci"),
        ContractData::builder(OWNER),
    )?;

    assert_eq!(session.call::<u32, u64>(id, "nth", &0)?.data, 1);
    assert_eq!(session.call::<u32, u64>(id, "nth", &1)?.data, 1);
    assert_eq!(session.call::<u32, u64>(id, "nth", &2)?.data, 2);
    assert_eq!(session.call::<u32, u64>(id, "nth", &3)?.data, 3);
    assert_eq!(session.call::<u32, u64>(id, "nth", &4)?.data, 5);

    Ok(())
}
