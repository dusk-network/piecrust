// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{contract_bytecode, ContractData, Error, SessionData, VM};

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;

#[tokio::test(flavor = "multi_thread")]
pub async fn vm_center_events() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let eventer_id = session.deploy(
        contract_bytecode!("eventer"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    const EVENT_NUM: u32 = 5;

    let receipt =
        session.call::<_, ()>(eventer_id, "emit_events", &EVENT_NUM, LIMIT)?;

    let events = receipt.events;
    assert_eq!(events.len() as u32, EVENT_NUM);

    for i in 0..EVENT_NUM {
        let index = i as usize;
        assert_eq!(events[index].source, eventer_id);
        assert_eq!(events[index].topic, "number");
        assert_eq!(events[index].data, i.to_le_bytes());
    }

    let receipt = session.call::<_, ()>(
        eventer_id,
        "emit_events_raw",
        &EVENT_NUM,
        LIMIT,
    )?;

    let events = receipt.events;
    assert_eq!(events.len() as u32, EVENT_NUM);

    for i in 0..EVENT_NUM {
        let index = i as usize;
        assert_eq!(events[index].source, eventer_id);
        assert_eq!(events[index].topic, "number");
        assert_eq!(events[index].data, i.to_le_bytes());
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
pub async fn event_costs() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let eventer_id = session.deploy(
        contract_bytecode!("eventer"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    // This call is to "prime" the contract
    let _ = session.call::<_, (u64, u64)>(
        eventer_id,
        "emit_input",
        &vec![1u8; 100],
        LIMIT,
    )?;

    let mut costs = vec![];

    for size in (4..=40).step_by(4) {
        let input = vec![1u8; size];
        let (spent_before, spent_after) = session
            .call::<_, (u64, u64)>(eventer_id, "emit_input", &input, LIMIT)?
            .data;
        let cost = spent_after - spent_before;
        print!("{cost} ");
        costs.push(cost);
    }

    // cost grows linearly with the amount of bytes processed, at a predictable
    // rate.
    //
    // NOTE: it is not possible to directly test emission costs, unless this is
    //       externally configurable
    let mut cost_diffs = Vec::with_capacity(costs.len() - 1);
    for i in 0..costs.len() - 1 {
        cost_diffs.push(costs[i + 1] - costs[i]);
    }
    let (ref_cost_diff, cost_diffs) = cost_diffs.split_first().unwrap();
    for cost_diff in cost_diffs {
        assert_eq!(
            cost_diff, ref_cost_diff,
            "cost should grow at a linear rate"
        );
    }

    Ok(())
}
