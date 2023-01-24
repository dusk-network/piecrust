// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{module_bytecode, Error, VM};
use piecrust_uplink::ModuleId;

#[test]
pub fn deploy_with_id() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;

    let bytecode = module_bytecode!("counter");
    let some_id = [1u8; 32];
    let module_id = ModuleId::from(some_id);
    let mut session = vm.session();
    session.deploy_with_id(module_id, bytecode)?;

    assert_eq!(
        session.query::<(), i64>(module_id, "read_value", &())?,
        0xfc
    );

    session.transact::<(), ()>(module_id, "increment", &())?;

    assert_eq!(
        session.query::<(), i64>(module_id, "read_value", &())?,
        0xfd
    );

    Ok(())
}