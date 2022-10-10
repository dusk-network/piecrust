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
