// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{module_bytecode, CommitId, Error, VM};

#[test]
pub fn state_root_calculation() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;
    let mut session = vm.session();
    let id_1 = session.deploy(module_bytecode!("counter"))?;

    session.transact::<(), ()>(id_1, "increment", &())?;
    let _commit = session.commit()?;

    let mut session = vm.session();
    let id_2 = session.deploy(module_bytecode!("box"))?;
    session.transact::<i16, ()>(id_2, "set", &0x11)?;
    let _commit = session.commit()?;

    let mut session = vm.session();
    let root_1 = session.root(false)?;

    let id_1 = session.deploy(module_bytecode!("counter"))?;
    session.transact::<(), ()>(id_1, "increment", &())?;

    let root_2 = session.root(false)?;

    // not committed changes do not cause the root change
    assert_eq!(root_1, root_2);

    let _commit = session.commit()?;
    let mut session = vm.session();

    let root_3 = session.root(false)?;

    // committed changes cause the root change
    assert_ne!(root_2, root_3);
    Ok(())
}

#[test]
pub fn initial_state_root() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;
    let mut session = vm.session();
    let id_1 = session.deploy(module_bytecode!("counter"))?;
    let id_2 = session.deploy(module_bytecode!("box"))?;

    session.transact::<(), ()>(id_1, "increment", &())?;
    session.transact::<i16, ()>(id_2, "set", &0x11)?;

    let root = session.root(false)?;
    // without commit, the root is initially set to zero
    assert_eq!(root, [0u8; 32]);
    Ok(())
}

#[test]
pub fn state_root_persist_restore() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;
    let mut session = vm.session();
    let id_1 = session.deploy(module_bytecode!("counter"))?;
    let id_2 = session.deploy(module_bytecode!("box"))?;

    session.transact::<(), ()>(id_1, "increment", &())?;
    session.transact::<i16, ()>(id_2, "set", &0x11)?;
    let _commit = session.commit()?;
    let mut session = vm.session();

    let root_1 = session.root(true)?;

    let mut session = vm.session();
    let id_1 = session.deploy(module_bytecode!("counter"))?;
    let id_2 = session.deploy(module_bytecode!("box"))?;
    session.transact::<(), ()>(id_1, "increment", &())?;
    session.transact::<i16, ()>(id_2, "set", &0x13)?;
    let _commit = session.commit()?;
    let mut session = vm.session();

    let root_2 = session.root(true)?;

    let mut session = vm.session();
    let id_1 = session.deploy(module_bytecode!("counter"))?;
    let id_2 = session.deploy(module_bytecode!("box"))?;
    assert_eq!(session.query::<(), i64>(id_1, "read_value", &())?, 0xfe);
    assert_eq!(
        session.query::<_, Option<i16>>(id_2, "get", &())?,
        Some(0x13)
    );

    let mut session = vm.session();
    let id_1 = session.deploy(module_bytecode!("counter"))?;
    let id_2 = session.deploy(module_bytecode!("box"))?;
    session.restore(&CommitId::from_bytes(root_1))?;

    assert_eq!(session.query::<(), i64>(id_1, "read_value", &())?, 0xfd);
    assert_eq!(
        session.query::<_, Option<i16>>(id_2, "get", &())?,
        Some(0x11)
    );

    session.restore(&CommitId::from_bytes(root_2))?;

    assert_eq!(session.query::<(), i64>(id_1, "read_value", &())?, 0xfe);
    assert_eq!(
        session.query::<_, Option<i16>>(id_2, "get", &())?,
        Some(0x13)
    );

    Ok(())
}
