// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{
    ContractData, ContractError, ContractId, Error, SessionData, VM,
    contract_bytecode,
};

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;

#[test]
pub fn deploy_with_id() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let bytecode = contract_bytecode!("counter");
    let some_id = [1u8; 32];
    let contract_id = ContractId::from(some_id);
    let mut session = vm.session(SessionData::builder())?;
    session.deploy::<_, (), _>(
        bytecode,
        ContractData::builder()
            .owner(OWNER)
            .contract_id(contract_id),
        LIMIT,
    )?;

    assert_eq!(
        session
            .call::<_, i64>(contract_id, "read_value", &(), LIMIT)?
            .data,
        0xfc
    );

    session.call::<_, ()>(contract_id, "increment", &(), LIMIT)?;

    assert_eq!(
        session
            .call::<_, i64>(contract_id, "read_value", &(), LIMIT)?
            .data,
        0xfd
    );

    Ok(())
}

#[test]
fn call_non_deployed() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let bytecode = contract_bytecode!("double_counter");
    let counter_id = ContractId::from_bytes([1; 32]);
    let mut session = vm.session(SessionData::builder())?;
    session.deploy::<_, (), _>(
        bytecode,
        ContractData::builder().owner(OWNER).contract_id(counter_id),
        LIMIT,
    )?;

    let (value, _) = session
        .call::<_, (i64, i64)>(counter_id, "read_values", &(), LIMIT)?
        .data;
    assert_eq!(value, 0xfc);

    let bogus_id = ContractId::from_bytes([255; 32]);
    let r = session
        .call::<_, Result<(), ContractError>>(
            counter_id,
            "increment_left_and_call",
            &bogus_id,
            LIMIT,
        )?
        .data;

    assert!(matches!(r, Err(ContractError::DoesNotExist)));

    let (value, _) = session
        .call::<_, (i64, i64)>(counter_id, "read_values", &(), LIMIT)?
        .data;
    assert_eq!(value, 0xfd);

    Ok(())
}

#[test]
fn contract_id_position_collision_is_rejected() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let id_a = ContractId::from_bytes([0; 32]);

    // Both IDs map to the same contract Merkle position: id_a sums to zero,
    // while id_b sums to 1 + u32::MAX, which wraps back to zero.
    let mut id_b_bytes = [0u8; 32];
    id_b_bytes[0..4].copy_from_slice(&1u32.to_le_bytes());
    id_b_bytes[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    let id_b = ContractId::from_bytes(id_b_bytes);

    assert_ne!(id_a, id_b);

    session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER).contract_id(id_a),
        LIMIT,
    )?;

    let err = session
        .deploy::<_, (), _>(
            contract_bytecode!("box"),
            ContractData::builder().owner(OWNER).contract_id(id_b),
            LIMIT,
        )
        .expect_err("colliding contract id should be rejected");

    assert!(matches!(
        err,
        Error::ContractPositionCollision {
            contract_id,
            pos: 0,
        } if contract_id == id_b
    ));

    Ok(())
}

#[test]
fn contract_id_position_collision_with_base_is_rejected() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let id_a = ContractId::from_bytes([0; 32]);

    // Both IDs map to the same contract Merkle position: id_a sums to zero,
    // while id_b sums to 1 + u32::MAX, which wraps back to zero.
    let mut id_b_bytes = [0u8; 32];
    id_b_bytes[0..4].copy_from_slice(&1u32.to_le_bytes());
    id_b_bytes[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    let id_b = ContractId::from_bytes(id_b_bytes);

    assert_ne!(id_a, id_b);

    let mut session = vm.session(SessionData::builder())?;
    session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER).contract_id(id_a),
        LIMIT,
    )?;
    let root = session.commit()?;

    let mut session = vm.session(SessionData::builder().base(root))?;
    let err = session
        .deploy::<_, (), _>(
            contract_bytecode!("box"),
            ContractData::builder().owner(OWNER).contract_id(id_b),
            LIMIT,
        )
        .expect_err("contract id colliding with base should be rejected");

    assert!(matches!(
        err,
        Error::ContractPositionCollision {
            contract_id,
            pos: 0,
        } if contract_id == id_b
    ));

    Ok(())
}
