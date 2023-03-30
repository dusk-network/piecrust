// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{module_bytecode, DeployData, Error, VM};

const OWNER: [u8; 32] = [0u8; 32];

#[test]
#[ignore]
fn fallible_read_write_panic() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.genesis_session();

    let id = session.deploy(
        module_bytecode!("fallible_counter"),
        DeployData::builder(OWNER),
    )?;

    session.transact::<bool, ()>(id, "increment", &false)?;

    assert_eq!(session.query::<(), i64>(id, "read_value", &())?, 0xfd);

    let err = session
        .transact::<bool, ()>(id, "increment", &true)
        .is_err();

    assert!(err, "execution failed");

    assert_eq!(
        session.query::<(), i64>(id, "read_value", &())?,
        0xfd,
        "should remain unchanged, since panics revert any changes"
    );

    Ok(())
}
