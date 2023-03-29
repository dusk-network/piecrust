// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{module_bytecode, DeployData, Error, VM};

const OWNER: [u8; 32] = [0u8; 32];

#[test]
pub fn height() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.genesis_session();

    let id = session
        .deploy(module_bytecode!("everest"), DeployData::<()>::from(OWNER))?;

    for h in 0u64..1024 {
        session.set_meta("height", h);
        let height: Option<u64> = session.transact(id, "get_height", &())?;
        assert_eq!(height.unwrap(), h);
    }

    Ok(())
}
#[test]
pub fn meta_data_optionality() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.genesis_session();
    let id = session
        .deploy(module_bytecode!("everest"), DeployData::<()>::from(OWNER))?;
    let height: Option<u64> = session.transact(id, "get_height", &())?;
    assert!(height.is_none());
    Ok(())
}
