// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{module_bytecode, Error, VM};

#[ignore]
fn constructor() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.genesis_session();

    let id = session.deploy(module_bytecode!("constructor"))?;

    assert_eq!(session.query::<(), u8>(id, "read_value", &())?, 0x50);

    // perform transaction and make sure that it fails because the contract is
    // not initialized yet
    let result = session.transact::<(), ()>(id, "increment", &());
    assert!(
        result.is_err(),
        "transaction on not initialized module should fail"
    );

    // initialize contract with some specific data
    session.init::<u8>(id, &0xab)?;

    // we should not be able to initialize contract more than once
    let result = session.init::<u8>(id, &0xaa);
    assert!(
        result.is_err(),
        "initializing more than once should not be allowed"
    );

    // perform transaction again, this time it should succeed
    session.transact::<(), ()>(id, "increment", &())?;

    // and make sure it performed ok
    assert_eq!(session.query::<(), u8>(id, "read_value", &())?, 0xac);

    // initialized state should live through across session boundaries
    let commit_id = session.commit()?;
    let mut session = vm.session(commit_id)?;
    assert_eq!(session.query::<(), u8>(id, "read_value", &())?, 0xac);

    // state of being initialized should live through across session boundaries
    let result = session.init::<u8>(id, &0xae);
    assert!(
        result.is_err(),
        "initializing more than once should not be allowed even in another session"
    );

    Ok(())
}
