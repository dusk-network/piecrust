// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{module_bytecode, DeployData, Error, VM};

const OWNER: [u8; 32] = [0u8; 32];

#[test]
pub fn vm_center_events() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.genesis_session();

    let eventer_id =
        session.deploy(module_bytecode!("eventer"), DeployData::from(OWNER))?;

    const EVENT_NUM: u32 = 5;

    session.transact(eventer_id, "emit_events", &EVENT_NUM)?;

    let events = session.take_events();
    assert_eq!(events.len() as u32, EVENT_NUM);

    for i in 0..EVENT_NUM {
        let index = i as usize;
        assert_eq!(events[index].source(), eventer_id);
        assert_eq!(events[index].data(), i.to_le_bytes());
    }

    Ok(())
}
