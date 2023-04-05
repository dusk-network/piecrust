// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{module_bytecode, CallData, Error, ModuleData, SessionData, VM};

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 65_536;

#[test]
pub fn fibo() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.genesis_session(SessionData::new());

    let id = session.deploy(
        module_bytecode!("fibonacci"),
        ModuleData::builder(OWNER),
        &CallData::build(LIMIT),
    )?;

    let call_data = CallData::build(LIMIT);
    assert_eq!(session.query::<u32, u64>(id, "nth", &0, &call_data)?, 1);
    assert_eq!(session.query::<u32, u64>(id, "nth", &1, &call_data)?, 1);
    assert_eq!(session.query::<u32, u64>(id, "nth", &2, &call_data)?, 2);
    assert_eq!(session.query::<u32, u64>(id, "nth", &3, &call_data)?, 3);
    assert_eq!(session.query::<u32, u64>(id, "nth", &4, &call_data)?, 5);

    Ok(())
}
