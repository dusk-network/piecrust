// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{contract_bytecode, ContractData, Error, SessionData, VM};
use piecrust_uplink::ContractId;

const LIMIT: u64 = 1_000_000;

#[test]
fn metadata() -> Result<(), Error> {
    const EXPECTED_OWNER: [u8; 33] = [3u8; 33];

    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let id = session.deploy(
        contract_bytecode!("metadata"),
        ContractData::builder().owner(EXPECTED_OWNER),
        LIMIT,
    )?;

    // owner should be available after deployment
    let owner = session
        .call::<_, [u8; 33]>(id, "read_owner", &(), LIMIT)?
        .data;
    let self_id = session
        .call::<_, ContractId>(id, "read_id", &(), LIMIT)?
        .data;
    assert_eq!(owner, EXPECTED_OWNER);
    assert_eq!(self_id, id);

    // owner should live across session boundaries
    let commit_id = session.commit()?;
    let mut session = vm.session(SessionData::builder().base(commit_id))?;
    let owner = session
        .call::<_, [u8; 33]>(id, "read_owner", &(), LIMIT)?
        .data;
    let self_id = session
        .call::<_, ContractId>(id, "read_id", &(), LIMIT)?
        .data;
    assert_eq!(owner, EXPECTED_OWNER);
    assert_eq!(self_id, id);

    Ok(())
}

#[test]
fn owner_of() -> Result<(), Error> {
    const EXPECTED_OWNER_0: [u8; 33] = [3u8; 33];
    const EXPECTED_OWNER_1: [u8; 33] = [4u8; 33];

    const CONTRACT_ID_0: ContractId = ContractId::from_bytes([1; 32]);
    const CONTRACT_ID_1: ContractId = ContractId::from_bytes([2; 32]);
    const CONTRACT_ID_2: ContractId = ContractId::from_bytes([3; 32]);

    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    session.deploy(
        contract_bytecode!("metadata"),
        ContractData::builder()
            .owner(EXPECTED_OWNER_0)
            .contract_id(CONTRACT_ID_0),
        LIMIT,
    )?;
    session.deploy(
        contract_bytecode!("metadata"),
        ContractData::builder()
            .owner(EXPECTED_OWNER_1)
            .contract_id(CONTRACT_ID_1),
        LIMIT,
    )?;

    let owner = session
        .call::<_, Option<[u8; 33]>>(
            CONTRACT_ID_0,
            "read_owner_of",
            &CONTRACT_ID_1,
            LIMIT,
        )?
        .data;

    assert_eq!(
        owner,
        Some(EXPECTED_OWNER_1),
        "The first contract should think the second contract has the correct owner"
    );

    let owner = session
        .call::<_, Option<[u8; 33]>>(
            CONTRACT_ID_1,
            "read_owner_of",
            &CONTRACT_ID_0,
            LIMIT,
        )?
        .data;

    assert_eq!(
        owner,
        Some(EXPECTED_OWNER_0),
        "The second contract should think the first contract has the correct owner"
    );

    let owner = session
        .call::<_, Option<[u8; 33]>>(
            CONTRACT_ID_0,
            "read_owner_of",
            &CONTRACT_ID_2,
            LIMIT,
        )?
        .data;

    assert_eq!(
        owner,
        None,
        "The first contract should think that the owner of a non-existing contract is None"
    );

    Ok(())
}

#[test]
fn free_limit_and_price_hint_of() -> Result<(), Error> {
    const OWNER_0: [u8; 33] = [3u8; 33];
    const OWNER_1: [u8; 33] = [4u8; 33];

    const EXPECTED_FREE_LIMIT_0: u64 = 10_000_000;
    const EXPECTED_FREE_LIMIT_1: u64 = 20_000_000;
    const EXPECTED_FREE_PRICE_HINT_0: (u64, u64) = (2, 1);
    const EXPECTED_FREE_PRICE_HINT_1: (u64, u64) = (3, 1);

    const CONTRACT_ID_0: ContractId = ContractId::from_bytes([1; 32]);
    const CONTRACT_ID_1: ContractId = ContractId::from_bytes([2; 32]);
    const CONTRACT_ID_2: ContractId = ContractId::from_bytes([3; 32]);

    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    session.deploy(
        contract_bytecode!("metadata"),
        ContractData::builder()
            .owner(OWNER_0)
            .contract_id(CONTRACT_ID_0)
            .free_limit(EXPECTED_FREE_LIMIT_0)
            .free_price_hint(EXPECTED_FREE_PRICE_HINT_0),
        LIMIT,
    )?;
    session.deploy(
        contract_bytecode!("metadata"),
        ContractData::builder()
            .owner(OWNER_1)
            .contract_id(CONTRACT_ID_1)
            .free_limit(EXPECTED_FREE_LIMIT_1)
            .free_price_hint(EXPECTED_FREE_PRICE_HINT_1),
        LIMIT,
    )?;

    let free_limit = session
        .call::<_, Option<u64>>(
            CONTRACT_ID_0,
            "read_free_limit_of",
            &CONTRACT_ID_1,
            LIMIT,
        )?
        .data;

    assert_eq!(
        free_limit,
        Some(EXPECTED_FREE_LIMIT_1),
        "contract 0 should think that contract 1 has the correct free limit"
    );

    let free_limit = session
        .call::<_, Option<u64>>(
            CONTRACT_ID_1,
            "read_free_limit_of",
            &CONTRACT_ID_0,
            LIMIT,
        )?
        .data;

    assert_eq!(
        free_limit,
        Some(EXPECTED_FREE_LIMIT_0),
        "contract 1 should think that contract 0 has the correct free limit"
    );

    let free_limit = session
        .call::<_, Option<u64>>(
            CONTRACT_ID_0,
            "read_free_limit_of",
            &CONTRACT_ID_2,
            LIMIT,
        )?
        .data;

    assert_eq!(
        free_limit,
        None,
        "contract 0 should think that the free_limit of a non-existing contract is None" );

    let free_price_hint = session
        .call::<_, Option<(u64, u64)>>(
            CONTRACT_ID_0,
            "read_free_price_hint_of",
            &CONTRACT_ID_1,
            LIMIT,
        )?
        .data;

    assert_eq!(
        free_price_hint,
        Some(EXPECTED_FREE_PRICE_HINT_1),
        "contract 0 should think that contract 1 has the correct free price hint" );

    let free_price_hint = session
        .call::<_, Option<(u64, u64)>>(
            CONTRACT_ID_1,
            "read_free_price_hint_of",
            &CONTRACT_ID_0,
            LIMIT,
        )?
        .data;

    assert_eq!(
        free_price_hint,
        Some(EXPECTED_FREE_PRICE_HINT_0),
        "contract 1 should think that contract 0 has the correct free price
    hint"
    );

    let free_price_hint = session
        .call::<_, Option<(u64, u64)>>(
            CONTRACT_ID_0,
            "read_free_price_hint_of",
            &CONTRACT_ID_2,
            LIMIT,
        )?
        .data;

    assert_eq!(
        free_price_hint,
        None,
        "contract 0 should think that the free price hint of a non-existing contract is none" );

    Ok(())
}
