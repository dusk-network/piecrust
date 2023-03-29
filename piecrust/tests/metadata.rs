// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{module_bytecode, DeployData, Error, VM};
use piecrust_uplink::ModuleId;

#[test]
fn metadata() -> Result<(), Error> {
    const EXPECTED_OWNER: [u8; 32] = [3u8; 32];

    let vm = VM::ephemeral()?;

    let mut session = vm.genesis_session();

    let id = session.deploy(
        module_bytecode!("metadata"),
        DeployData::<()>::from(EXPECTED_OWNER),
    )?;

    // owner should be available after deployment
    let owner = session.query::<(), [u8; 32]>(id, "read_owner", &())?;
    let self_id = session.query::<(), ModuleId>(id, "read_id", &())?;
    assert_eq!(owner, EXPECTED_OWNER);
    assert_eq!(self_id, id);

    // owner should live across session boundaries
    let commit_id = session.commit()?;
    let mut session = vm.session(commit_id)?;
    let owner = session.query::<(), [u8; 32]>(id, "read_owner", &())?;
    let self_id = session.query::<(), ModuleId>(id, "read_id", &())?;
    assert_eq!(owner, EXPECTED_OWNER);
    assert_eq!(self_id, id);

    Ok(())
}
