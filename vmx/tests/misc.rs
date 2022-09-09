// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use vmx::{module_bytecode, Error, VM};

#[test]
fn nstack_push_pop() -> Result<(), Error> {
    let vm = VM::new();
    let mut session = vm.session_mut(None)?;

    let module_id = session.deploy(module_bytecode!("stack"))?;

    let val = 42;

    session.transact::<i32, ()>(module_id, "push", val)?;

    let len = session.query::<_, i32>(module_id, "len", ())?;
    assert_eq!(len, 1);

    let popped = session.transact::<_, Option<i32>>(module_id, "pop", ())?;
    let len = session.query::<_, i32>(module_id, "len", ())?;

    assert_eq!(len, 0);
    assert_eq!(popped, Some(val));

    Ok(())
}

#[test]
fn nstack_multi_push_pop() -> Result<(), Error> {
    let vm = VM::new();
    let mut session = vm.session_mut(None)?;

    let module_id = session.deploy(module_bytecode!("stack"))?;

    const N: i32 = 1_000;

    for i in 0..N {
        session.transact::<i32, ()>(module_id, "push", i)?;
        let len = session.query::<_, i32>(module_id, "len", ())?;

        assert_eq!(len, i + 1);
    }

    for i in (0..N).rev() {
        let popped =
            session.transact::<(), Option<i32>>(module_id, "pop", ())?;
        let len = session.query::<_, i32>(module_id, "len", ())?;

        assert_eq!(len, i);
        assert_eq!(popped, Some(i));
    }

    let popped = session.transact::<(), Option<i32>>(module_id, "pop", ())?;
    assert_eq!(popped, None);

    Ok(())
}

#[test]
fn fibonacci() -> Result<(), Error> {
    let vm = VM::new();
    let mut session = vm.session_mut(None)?;

    let module_id = session.deploy(module_bytecode!("fibonacci"))?;

    assert_eq!(session.query::<u32, u64>(module_id, "nth", 0)?, 1);
    assert_eq!(session.query::<u32, u64>(module_id, "nth", 1)?, 1);
    assert_eq!(session.query::<u32, u64>(module_id, "nth", 2)?, 2);
    assert_eq!(session.query::<u32, u64>(module_id, "nth", 3)?, 3);
    assert_eq!(session.query::<u32, u64>(module_id, "nth", 4)?, 5);

    Ok(())
}

#[test]
fn vector_push_pop() -> Result<(), Error> {
    let vm = VM::new();
    let mut session = vm.session_mut(None)?;

    let module_id = session.deploy(module_bytecode!("vector"))?;

    const N: i16 = 128;

    for i in 0..N {
        session.transact::<_, ()>(module_id, "push", i)?;
    }

    for i in (0..N).rev() {
        let popped =
            session.transact::<(), Option<i16>>(module_id, "pop", ())?;
        assert_eq!(popped, Some(i));
    }

    Ok(())
}
