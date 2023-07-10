// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{contract_bytecode, ContractData, Error, SessionData, VM};
use piecrust_uplink::EventTarget;

const OWNER: [u8; 32] = [0u8; 32];

#[test]
pub fn vm_center_events() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let eventer_id = session
        .deploy(contract_bytecode!("eventer"), ContractData::builder(OWNER))?;

    const EVENT_NUM: u32 = 5;

    let receipt =
        session.call::<_, ()>(eventer_id, "emit_events", &EVENT_NUM)?;

    let events = receipt.events;
    assert_eq!(events.len() as u32, EVENT_NUM);

    for i in 0..EVENT_NUM {
        let index = i as usize;
        assert_eq!(events[index].topic, "number");
        assert_eq!(events[index].target, EventTarget::Contract(eventer_id));
        assert_eq!(events[index].data, i.to_le_bytes());
    }

    Ok(())
}
