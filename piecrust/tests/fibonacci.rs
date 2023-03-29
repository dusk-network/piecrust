// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{module_bytecode, DeployData, Error, VM};

const OWNER: [u8; 32] = [0u8; 32];

#[test]
pub fn fibo() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.genesis_session();

    let id = session
        .deploy(module_bytecode!("fibonacci"), DeployData::from(OWNER))?;

    assert_eq!(session.query::<u32, u64>(id, "nth", &0)?, 1);
    assert_eq!(session.query::<u32, u64>(id, "nth", &1)?, 1);
    assert_eq!(session.query::<u32, u64>(id, "nth", &2)?, 2);
    assert_eq!(session.query::<u32, u64>(id, "nth", &3)?, 3);
    assert_eq!(session.query::<u32, u64>(id, "nth", &4)?, 5);

    Ok(())
}
