// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use vmx::{module_bytecode, Error, VM};

#[test]
fn counter_read_simple() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;
    let id = vm.deploy(module_bytecode!("counter"))?;

    assert_eq!(vm.query::<(), i64>(id, "read_value", ())?, 0xfc);

    Ok(())
}

#[test]
fn counter_read_write_simple() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;
    let id = vm.deploy(module_bytecode!("counter"))?;

    let mut session = vm.session();

    assert_eq!(session.query::<(), i64>(id, "read_value", ())?, 0xfc);

    session.transact::<(), ()>(id, "increment", ())?;

    assert_eq!(session.query::<(), i64>(id, "read_value", ())?, 0xfd);

    Ok(())
}

#[test]
fn counter_read_write_session() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;
    let id = vm.deploy(module_bytecode!("counter"))?;

    {
        let mut session = vm.session();

        assert_eq!(session.query::<(), i64>(id, "read_value", ())?, 0xfc);

        session.transact::<(), ()>(id, "increment", ())?;

        assert_eq!(session.query::<(), i64>(id, "read_value", ())?, 0xfd);
    }

    // mutable session dropped without committing.
    // old counter value still accessible.

    assert_eq!(vm.query::<(), i64>(id, "read_value", ())?, 0xfc);

    let mut other_session = vm.session();

    other_session.transact::<(), ()>(id, "increment", ())?;

    let _commit_id = vm.commit(&id)?;

    // session committed, new value accessible

    assert_eq!(vm.query::<(), i64>(id, "read_value", ())?, 0xfd);

    Ok(())
}

#[test]
fn counter_commit_restore() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;
    let id = vm.deploy(module_bytecode!("counter"))?;

    // commit 1
    let mut session_1 = vm.session();

    assert_eq!(session_1.query::<(), i64>(id, "read_value", ())?, 0xfc);

    session_1.transact::<(), ()>(id, "increment", ())?;

    let commit_1 = vm.commit(&id)?;

    // commit 2
    let mut session_2 = vm.session();

    assert_eq!(session_2.query::<(), i64>(id, "read_value", ())?, 0xfd);

    session_2.transact::<(), ()>(id, "increment", ())?;
    session_2.transact::<(), ()>(id, "increment", ())?;

    let commit_2 = vm.commit(&id)?;

    assert_eq!(session_2.query::<(), i64>(id, "read_value", ())?, 0xfe);

    // restore commit 1

    vm.restore(&id, &commit_1)?;

    assert_eq!(session_1.query::<(), i64>(id, "read_value", ())?, 0xfd);

    // restore commit 2

    vm.restore(&id, &commit_2)?;

    assert_eq!(session_1.query::<(), i64>(id, "read_value", ())?, 0xfe);

    Ok(())
}
