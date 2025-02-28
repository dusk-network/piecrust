// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

#![cfg(feature = "serde")]

use piecrust_uplink::{ContractId, Event, CONTRACT_ID_BYTES};
use rand::rngs::StdRng;
use rand::{RngCore, SeedableRng};
use serde::Serialize;

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

fn assert_canonical_json<T>(
    input: &T,
    expected: &str,
) -> Result<String, Box<dyn std::error::Error>>
where
    T: ?Sized + Serialize,
{
    let serialized = serde_json::to_string(input)?;
    let input_canonical: serde_json::Value = serialized.parse()?;
    let expected_canonical: serde_json::Value = expected.parse()?;
    assert_eq!(input_canonical, expected_canonical);
    Ok(serialized)
}

#[test]
fn serde_contract_id() -> Result<(), Box<dyn std::error::Error>> {
    let mut rng = StdRng::seed_from_u64(0xdead);
    let id: ContractId = rand_contract_id(&mut rng);
    let ser = assert_canonical_json(
        &id,
        "\"c48dcb7e531ccc3b334ae122d4fd40e242e7d8a85fdb82bd4c9e9621a9a60d44\"",
    )?;
    let deser: ContractId = serde_json::from_str(&ser)?;
    assert_eq!(id, deser);
    Ok(())
}

#[test]
fn serde_event() -> Result<(), Box<dyn std::error::Error>> {
    let mut rng = StdRng::seed_from_u64(0xbeef);
    let event = rand_event(&mut rng);
    let ser = assert_canonical_json(&event, include_str!("./event.json"))?;
    let deser: Event = serde_json::from_str(&ser)?;
    assert_eq!(event, deser);
    Ok(())
}

#[test]
fn serde_wrong_encoded() {
    let wrong_encoded = "\"wrong-encoded\"";

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
