// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{contract_bytecode, ContractData, Error, SessionData, VM};

const OWNER: [u8; 32] = [0u8; 32];

#[test]
#[ignore]
fn fallible_read_write_panic() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let id = session.deploy(
        contract_bytecode!("fallible_counter"),
        ContractData::builder(OWNER),
    )?;

    session.call::<bool, ()>(id, "increment", &false)?;

    assert_eq!(session.call::<(), i64>(id, "read_value", &())?, 0xfd);

    let err = session.call::<bool, ()>(id, "increment", &true).is_err();

    assert!(err, "execution failed");

    assert_eq!(
        session.call::<(), i64>(id, "read_value", &())?,
        0xfd,
        "should remain unchanged, since panics revert any changes"
    );

    Ok(())
}
