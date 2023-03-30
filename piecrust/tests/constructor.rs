// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{module_bytecode, DeployData, Error, VM};

const CONTRACT_INIT_METHOD: &str = "init";
const OWNER: [u8; 32] = [0u8; 32];

#[test]
fn constructor() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.genesis_session();

    let id = session.deploy(
        module_bytecode!("constructor"),
        DeployData::build(OWNER).constructor_arg(&0xabu8),
    )?;

    assert_eq!(session.query::<(), u8>(id, "read_value", &())?, 0xab);

    // perform transaction and make sure that the contract works as expected
    session.transact::<(), ()>(id, "increment", &())?;
    assert_eq!(session.query::<(), u8>(id, "read_value", &())?, 0xac);

    // we should not be able to call init directly
    let result = session.transact::<u8, ()>(id, CONTRACT_INIT_METHOD, &0xaa);
    assert!(
        result.is_err(),
        "calling init directly as transaction should not be allowed"
    );
    // we should not be able to call init as query neither
    let result = session.query::<u8, ()>(id, CONTRACT_INIT_METHOD, &0xaa);
    assert!(
        result.is_err(),
        "calling init directly as query should not be allowed"
    );

    // make sure the state is still ok
    assert_eq!(session.query::<(), u8>(id, "read_value", &())?, 0xac);

    // initialized state should live through across session boundaries
    let commit_id = session.commit()?;
    let mut session = vm.session(commit_id)?;
    assert_eq!(session.query::<(), u8>(id, "read_value", &())?, 0xac);

    // not being able to call init directly should also be enforced across
    // session boundaries
    let result = session.transact::<u8, ()>(id, CONTRACT_INIT_METHOD, &0xae);
    assert!(
        result.is_err(),
        "calling init directly should never be allowed"
    );

    Ok(())
}

#[test]
fn missing_init() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.genesis_session();

    let result = session.deploy(
        module_bytecode!("counter"),
        DeployData::build(OWNER).constructor_arg(&0xabu8),
    );

    assert!(
        result.is_err(),
        "deploy with data when the 'init' method is not exported should fail with an error"
    );

    Ok(())
}