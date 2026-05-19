// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{ContractData, Error, SessionData, VM, contract_bytecode};
use piecrust_uplink::ContractError;

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;

#[test]
pub fn reverted_icc_discards_events() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let (eventer_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("eventer"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    let (reverter_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("event_reverter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    let (center_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let value: u32 = session.call(eventer_id, "read_value", &(), LIMIT)?.data;
    assert_eq!(value, 0);

    let eventer_arg = rkyv::to_bytes::<_, 256>(&eventer_id)
        .expect("eventer ID should serialize")
        .to_vec();
    let receipt = session.call::<_, Result<Vec<u8>, ContractError>>(
        center_id,
        "delegate_query_with_event",
        &(reverter_id, String::from("emit_then_panic"), eventer_arg),
        LIMIT,
    )?;

    assert!(
        matches!(receipt.data, Err(ContractError::Panic(_))),
        "the middle ICC should panic and be propagated as contract data"
    );

    let value: u32 = session.call(eventer_id, "read_value", &(), LIMIT)?.data;
    assert_eq!(value, 0, "eventer state should be reverted");

    assert!(
        receipt
            .events
            .iter()
            .any(|event| event.source == center_id
                && event.topic == "callcenter"),
        "the upper-layer callcenter event should remain in the receipt"
    );
    assert!(
        receipt
            .events
            .iter()
            .all(|event| event.source != eventer_id),
        "event(s) from a reverted ICC survived in the outer receipt"
    );
    assert_eq!(receipt.events.len(), 1);

    Ok(())
}
