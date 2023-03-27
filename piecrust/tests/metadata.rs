// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{module_bytecode, Error, VM};
use piecrust_uplink::ModuleMetadata;

#[test]
fn metadata() -> Result<(), Error> {
    const EXPECTED_OWNER: Option<&[u8; 32]> = None::<&[u8; 32]>;

    let vm = VM::ephemeral()?;

    let mut session = vm.genesis_session();

    let id = session.deploy(module_bytecode!("metadata"), None::<&()>)?;

    // owner should be available after deployment
    let metadata =
        session.query::<(), ModuleMetadata>(id, "read_metadata", &())?;
    assert_eq!(metadata.owner(), EXPECTED_OWNER);

    // owner should live across session boundaries
    let commit_id = session.commit()?;
    let mut session = vm.session(commit_id)?;
    let metadata =
        session.query::<(), ModuleMetadata>(id, "read_metadata", &())?;
    assert_eq!(metadata.owner(), EXPECTED_OWNER);

    Ok(())
}
