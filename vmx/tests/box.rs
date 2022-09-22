// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use vmx::{module_bytecode, Error, VM};

#[test]
pub fn box_set_get() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;

    let id = vm.deploy(module_bytecode!("box"))?;

    let value: Option<i16> = vm.query(id, "get", ())?;

    assert_eq!(value, None);

    println!("no problem until here 1");

    let mut session = vm.session();

    println!("no problem until here 2");
    session.transact::<i16, ()>(id, "set", 0x11)?;
    println!("no problem until here 3");
    session.commit(&id)?;

    let value = session.query::<_, Option<i16>>(id, "get", ())?;

    println!("no problem until here 3");
    assert_eq!(value, Some(0x11));

    Ok(())
}
