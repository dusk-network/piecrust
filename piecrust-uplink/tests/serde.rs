// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

#![cfg(feature = "serde")]

use piecrust_uplink::{ContractId, Event, CONTRACT_ID_BYTES};
use rand::rngs::StdRng;
use rand::{RngCore, SeedableRng};

fn rand_contract_id(rng: &mut StdRng) -> ContractId {
    let mut bytes = [0; CONTRACT_ID_BYTES];
    rng.fill_bytes(&mut bytes);
    bytes.into()
}

fn rand_event(rng: &mut StdRng) -> Event {
    let mut data = [0; 50];
    rng.fill_bytes(&mut data);
    Event {
        source: rand_contract_id(rng),
        topic: "a-contract-topic".into(),
        data: data.into(),
    }
}

#[test]
fn contract_id() {
    let mut rng = StdRng::seed_from_u64(0xdead);
    let id: ContractId = rand_contract_id(&mut rng);
    let ser = serde_json::to_string(&id).unwrap();
    let deser: ContractId = serde_json::from_str(&ser).unwrap();
    assert_eq!(id, deser);
}

#[test]
fn event() {
    let mut rng = StdRng::seed_from_u64(0xbeef);
    let event = rand_event(&mut rng);
    let ser = serde_json::to_string(&event).unwrap();
    let deser: Event = serde_json::from_str(&ser).unwrap();
    assert_eq!(event, deser);
}
