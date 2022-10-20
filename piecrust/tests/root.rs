// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{module_bytecode, Error, VM};

#[test]
pub fn state_root_calculation() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;
    let id_1 = vm.deploy(module_bytecode!("counter"))?;
    let id_2 = vm.deploy(module_bytecode!("box"))?;

    let mut session = vm.session();
    session.transact::<(), ()>(id_1, "increment", ())?;
    let _commit = session.commit()?;

    let mut session = vm.session();
    session.transact::<i16, ()>(id_2, "set", 0x11)?;
    let _commit = session.commit()?;

    let root_1 = vm.root(false)?;

    let mut session = vm.session();
    session.transact::<(), ()>(id_1, "increment", ())?;

    let root_2 = vm.root(false)?;

    // not committed changes do not cause the root to change
    assert_eq!(root_1, root_2);

    let _commit = session.commit()?;

    let root_3 = vm.root(false)?;

    // committed changes cause the root to change
    assert_ne!(root_2, root_3);
    Ok(())
}

#[test]
pub fn initial_state_root() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;
    let id_1 = vm.deploy(module_bytecode!("counter"))?;
    let id_2 = vm.deploy(module_bytecode!("box"))?;

    let mut session = vm.session();
    session.transact::<(), ()>(id_1, "increment", ())?;
    session.transact::<i16, ()>(id_2, "set", 0x11)?;

    let root = vm.root(false)?;
    assert_eq!(root, [0u8; 32]);
    Ok(())
}

#[test]
pub fn state_root_persist_restore() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;
    let id_1 = vm.deploy(module_bytecode!("counter"))?;
    let id_2 = vm.deploy(module_bytecode!("box"))?;

    let mut session = vm.session();
    session.transact::<(), ()>(id_1, "increment", ())?;
    session.transact::<i16, ()>(id_2, "set", 0x11)?;
    let _commit = session.commit()?;

    let root_1 = vm.root(true)?;

    let mut session = vm.session();
    session.transact::<(), ()>(id_1, "increment", ())?;
    session.transact::<i16, ()>(id_2, "set", 0x13)?;
    let _commit = session.commit()?;

    let root_2 = vm.root(true)?;

    let mut session = vm.session();
    assert_eq!(session.query::<(), i64>(id_1, "read_value", ())?, 0xfe);
    assert_eq!(
        session.query::<_, Option<i16>>(id_2, "get", ())?,
        Some(0x13)
    );

    let mut session = vm.session();

    vm.restore_root(&root_1)?;

    assert_eq!(session.query::<(), i64>(id_1, "read_value", ())?, 0xfd);
    assert_eq!(
        session.query::<_, Option<i16>>(id_2, "get", ())?,
        Some(0x11)
    );

    vm.restore_root(&root_2)?;

    assert_eq!(session.query::<(), i64>(id_1, "read_value", ())?, 0xfe);
    assert_eq!(
        session.query::<_, Option<i16>>(id_2, "get", ())?,
        Some(0x13)
    );

    Ok(())
}
