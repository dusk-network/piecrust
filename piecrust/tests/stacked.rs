// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{module_bytecode, Error, VM};

#[test]
pub fn push_pop() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;

    let id = vm.deploy(module_bytecode!("stack"))?;

    let val = 42;

    let mut session = vm.session();

    session.transact(id, "push", val)?;

    let len: u32 = session.query(id, "len", ())?;
    assert_eq!(len, 1);

    let popped: Option<i32> = session.transact(id, "pop", ())?;
    let len: i32 = session.query(id, "len", ())?;

    assert_eq!(len, 0);
    assert_eq!(popped, Some(val));

    Ok(())
}

#[test]
pub fn multi_push_pop() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;

    let id = vm.deploy(module_bytecode!("stack"))?;

    let mut session = vm.session();

    const N: i32 = 16;

    for i in 0..N {
        session.transact(id, "push", i)?;
        let len: i32 = session.query(id, "len", ())?;

        assert_eq!(len, i + 1);
    }

    for i in (0..N).rev() {
        let popped: Option<i32> = session.transact(id, "pop", ())?;
        let len: i32 = session.query(id, "len", ())?;

        assert_eq!(len, i);
        assert_eq!(popped, Some(i));
    }

    let popped: Option<i32> = session.transact(id, "pop", ())?;
    assert_eq!(popped, None);

    Ok(())
}
