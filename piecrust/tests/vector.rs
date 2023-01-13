// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{module_bytecode, Error, VM};

#[test]
pub fn vector_push_pop() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;

    let mut session = vm.session();

    let id = session.deploy(module_bytecode!("vector"))?;

    const N: usize = 128;

    for i in 0..N {
        session.transact::<_, ()>(id, "push", &(i as i16))?;
    }

    for i in 0..N {
        let popped: Option<i16> = session.transact(id, "pop", &())?;

        assert_eq!(popped, Some((N - i - 1) as i16));
    }

    let popped: Option<i16> = session.transact(id, "pop", &())?;

    assert_eq!(popped, None);

    Ok(())
}
