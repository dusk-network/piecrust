// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{module_bytecode, Error, VM};

#[test]
fn metadata() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.genesis_session();

    let id = session.deploy(module_bytecode!("metadata"), None::<&()>)?;

    let owner = session.query::<(), [u8; 32]>(id, "read_owner", &())?;
    println!("owner1 = {:x?}", owner);
    assert_eq!(owner, [3u8; 32]);

    // metadata should live through across session boundaries
    let commit_id = session.commit()?;
    let mut session = vm.session(commit_id)?;
    let owner = session.query::<(), [u8; 32]>(id, "read_owner", &())?;
    println!("owner2 = {:x?}", owner);
    assert_eq!(owner, [3u8; 32]);

    Ok(())
}
