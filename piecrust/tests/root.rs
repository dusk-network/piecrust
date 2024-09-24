// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{contract_bytecode, ContractData, Error, SessionData, VM};

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;

// #[test]
// pub fn state_root_calculation() -> Result<(), Error> {
//     let vm = VM::ephemeral()?;
//     let mut session = vm.session(None, SessionData::builder())?;
//     let id_1 = session.deploy(
//         None,
//         contract_bytecode!("counter"),
//         &(),
//         OWNER,
//         LIMIT,
//     )?;
//
//     session.call::<_, ()>(id_1, "increment", &(), LIMIT)?;
//
//     let root_1 = session.root();
//     let commit_1 = session.commit()?;
//
//     assert_eq!(
//         commit_1, root_1,
//         "The commit root is the same as the state root"
//     );
//
//     let mut session = vm.session(Some(commit_1), SessionData::builder())?;
//     let id_2 =
//         session.deploy(None, contract_bytecode!("box"), &(), OWNER, LIMIT)?;
//     session.call::<i16, ()>(id_2, "set", &0x11, LIMIT)?;
//     session.call::<_, ()>(id_1, "increment", &(), LIMIT)?;
//
//     let root_2 = session.root();
//     let commit_2 = session.commit()?;
//
//     assert_eq!(
//         commit_2, root_2,
//         "The commit root is the same as the state root"
//     );
//     assert_ne!(
//         root_1, root_2,
//         "The state root should change since the state changes"
//     );
//
//     let session = vm.session(Some(commit_2), SessionData::builder())?;
//     let root_3 = session.root();
//
//     assert_eq!(root_2, root_3, "The root of a session should be the same if
// no modifications were made");     Ok(())
// }
