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
fn serde_contract_id() {
    let mut rng = StdRng::seed_from_u64(0xdead);
    let id: ContractId = rand_contract_id(&mut rng);
    let ser = serde_json::to_string(&id).unwrap();
    let deser: ContractId = serde_json::from_str(&ser).unwrap();
    assert_eq!(id, deser);
}

#[test]
fn serde_event() {
    let mut rng = StdRng::seed_from_u64(0xbeef);
    let event = rand_event(&mut rng);
    let ser = serde_json::to_string(&event).unwrap();
    let deser: Event = serde_json::from_str(&ser).unwrap();
    assert_eq!(event, deser);
}

#[test]
fn serde_wrong_encoded() {
    let wrong_encoded = "wrong-encoded";

    let contract_id: Result<ContractId, _> =
        serde_json::from_str(&wrong_encoded);
    assert!(contract_id.is_err());
}

#[test]
fn serde_too_long_encoded() {
    let length_33_enc = "\"e4ab9de40283a85d6ea0cd0120500697d8b01c71b7b4b520292252d20937000631\"";

    let contract_id: Result<ContractId, _> =
        serde_json::from_str(&length_33_enc);
    assert!(contract_id.is_err());
}

#[test]
fn serde_too_short_encoded() {
    let length_31_enc =
        "\"1751c37a1dca7aa4c048fcc6177194243edc3637bae042e167e4285945e046\"";

    let contract_id: Result<ContractId, _> =
        serde_json::from_str(&length_31_enc);
    assert!(contract_id.is_err());
}

#[test]
fn serde_event_fields() {
    let serde_json_string = "{\"source\":\"0000000000000000000000000000000000000000000000000000000000000000\",\"topic\":\"\",\"data\":\"\"}";
    let event = Event {
        source: ContractId::from_bytes([0; CONTRACT_ID_BYTES]),
        topic: String::new(),
        data: Vec::new(),
    };
    let ser = serde_json::to_string(&event).unwrap();
    assert_eq!(serde_json_string, ser);
}
