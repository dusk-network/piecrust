// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{module_bytecode, Error, VM};

#[test]
pub fn vm_center_events() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;

    let mut session = vm.session();

    let eventer_id = session.deploy(module_bytecode!("eventer"))?;

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