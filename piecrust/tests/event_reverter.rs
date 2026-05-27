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
pub fn reverted_icc_marks_events() -> Result<(), Error> {
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

    assert_eq!(receipt.events.len(), 3);
    assert_eq!(receipt.events[0].source, center_id);
    assert_eq!(receipt.events[0].topic, "callcenter-before");
    assert!(
        !receipt.events[0].reverted,
        "the pre-ICC callcenter event should not be marked as reverted"
    );

    assert_eq!(receipt.events[1].source, eventer_id);
    assert_eq!(receipt.events[1].topic, "number");
    assert!(
        receipt.events[1].reverted,
        "event emitted by the reverted ICC should be marked as reverted"
    );

    assert_eq!(receipt.events[2].source, center_id);
    assert_eq!(receipt.events[2].topic, "callcenter-after");
    assert!(
        !receipt.events[2].reverted,
        "the post-ICC callcenter event should not be marked as reverted"
    );

    Ok(())
}
