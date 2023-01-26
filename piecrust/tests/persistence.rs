// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{module_bytecode, Error, VM};

#[test]
fn session_commits_persistence() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;

    let commit_1;
    {
        let mut session = vm.session();
        let id_1 = session.deploy(module_bytecode!("counter"))?;
        let id_2 = session.deploy(module_bytecode!("box"))?;

        session.transact::<(), ()>(id_1, "increment", &())?;
        session.transact::<i16, ()>(id_2, "set", &0x11)?;
        assert_eq!(session.query::<(), i64>(id_1, "read_value", &())?, 0xfd);
        assert_eq!(
            session.query::<_, Option<i16>>(id_2, "get", &())?,
            Some(0x11)
        );
        commit_1 = session.commit()?;
    }

    let commit_2;
    {
        let mut session = vm.session();
        let id_1 = session.deploy(module_bytecode!("counter"))?;
        let id_2 = session.deploy(module_bytecode!("box"))?;

        session.transact::<(), ()>(id_1, "increment", &())?;
        session.transact::<i16, ()>(id_2, "set", &0x12)?;
        assert_eq!(session.query::<(), i64>(id_1, "read_value", &())?, 0xfe);
        assert_eq!(
            session.query::<_, Option<i16>>(id_2, "get", &())?,
            Some(0x12)
        );
        commit_2 = session.commit()?;
    }

    vm.persist()?;

    {
        let mut vm2 = VM::new(vm.base_path())?;
        let mut session = vm2.session();
        let id_1 = session.deploy(module_bytecode!("counter"))?;
        let id_2 = session.deploy(module_bytecode!("box"))?;

        session.restore(&commit_1)?;

        // check if both modules' state was restored
        assert_eq!(session.query::<(), i64>(id_1, "read_value", &())?, 0xfd);
        assert_eq!(
            session.query::<_, Option<i16>>(id_2, "get", &())?,
            Some(0x11)
        );
    }

    {
        let mut vm3 = VM::new(vm.base_path())?;
        let mut session = vm3.session();
        let id_1 = session.deploy(module_bytecode!("counter"))?;
        let id_2 = session.deploy(module_bytecode!("box"))?;

        session.restore(&commit_2)?;

        // check if both modules' state was restored
        assert_eq!(session.query::<(), i64>(id_1, "read_value", &())?, 0xfe);
        assert_eq!(
            session.query::<_, Option<i16>>(id_2, "get", &())?,
            Some(0x12)
        );
    }
    Ok(())
}

#[test]
fn modules_persistence() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;
    let mut session = vm.session();
    let id_1 = session.deploy(module_bytecode!("counter"))?;
    let id_2 = session.deploy(module_bytecode!("box"))?;

    session.transact::<(), ()>(id_1, "increment", &())?;
    session.transact::<i16, ()>(id_2, "set", &0x11)?;
    assert_eq!(session.query::<(), i64>(id_1, "read_value", &())?, 0xfd);
    assert_eq!(
        session.query::<_, Option<i16>>(id_2, "get", &())?,
        Some(0x11)
    );

    let commit_1 = session.commit()?;
    vm.persist();

    let mut vm2 = VM::new(vm.base_path())?;
    let mut session2 = vm2.session();
    let id_1 = session2.deploy(module_bytecode!("counter"))?;
    let id_2 = session2.deploy(module_bytecode!("box"))?;
    session2.restore(&commit_1);

    // check if both modules' state was restored
    assert_eq!(session2.query::<(), i64>(id_1, "read_value", &())?, 0xfd);
    assert_eq!(
        session2.query::<_, Option<i16>>(id_2, "get", &())?,
        Some(0x11)
    );
    Ok(())
}
