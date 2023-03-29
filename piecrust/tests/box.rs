// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{module_bytecode, Error, ModuleData, VM};

const OWNER: [u8; 32] = [0u8; 32];

#[test]
pub fn box_set_get() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.genesis_session();

    let id = session
        .deploy(module_bytecode!("box"), ModuleData::<()>::from(OWNER))?;

    let value: Option<i16> = session.query(id, "get", &())?;

    assert_eq!(value, None);

    session.transact::<i16, ()>(id, "set", &0x11)?;

    let value = session.query::<_, Option<i16>>(id, "get", &())?;

    assert_eq!(value, Some(0x11));

    Ok(())
}
