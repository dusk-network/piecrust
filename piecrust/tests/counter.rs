// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{module_bytecode, Error, VM};

#[test]
fn counter_read_simple() -> Result<(), Error> {
    // let vm = VM::ephemeral()?;
    let vm = VM::new("/tmp/001")?;

    let mut session = vm.genesis_session();

    let id = session.deploy(module_bytecode!("counter"))?;

    assert_eq!(session.query::<(), i64>(id, "read_value", &())?, 0xfc);

    Ok(())
}

#[test]
fn counter_read_write_simple() -> Result<(), Error> {
    // let vm = VM::ephemeral()?;
    let vm = VM::new("/tmp/001")?;

    let mut session = vm.genesis_session();

    let id = session.deploy(module_bytecode!("counter"))?;

    assert_eq!(session.query::<(), i64>(id, "read_value", &())?, 0xfc);

    session.transact::<(), ()>(id, "increment", &())?;

    assert_eq!(session.query::<(), i64>(id, "read_value", &())?, 0xfd);

    let commit_id = session.commit()?;
    println!("after first commit");
    let mut session = vm.session(commit_id)?;
    session.transact::<(), ()>(id, "increment", &())?;

    let commit_id = session.commit()?;
    println!("after second commit");
    let mut session = vm.session(commit_id)?;
    session.transact::<(), ()>(id, "increment", &())?;

    Ok(())
}
