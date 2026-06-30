// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{
    ContractData, ContractId, Error, SessionData, VM, contract_bytecode,
};

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;

#[test]
fn session_commits_persistence() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let id_1;
    let id_2;

    let commit_1;
    {
        let mut session = vm.session(SessionData::builder())?;
        (id_1, _) = session.deploy::<_, (), _>(
            contract_bytecode!("counter"),
            ContractData::builder().owner(OWNER),
            LIMIT,
        )?;
        (id_2, _) = session.deploy::<_, (), _>(
            contract_bytecode!("box"),
            ContractData::builder().owner(OWNER),
            LIMIT,
        )?;

        session.call::<_, ()>(id_1, "increment", &(), LIMIT)?;
        session.call::<i16, ()>(id_2, "set", &0x11, LIMIT)?;
        assert_eq!(
            session.call::<_, i64>(id_1, "read_value", &(), LIMIT)?.data,
            0xfd
        );
        assert_eq!(
            session
                .call::<_, Option<i16>>(id_2, "get", &(), LIMIT)?
                .data,
            Some(0x11)
        );
        commit_1 = session.commit()?;
    }

    let commit_2;
    {
        let mut session = vm.session(SessionData::builder().base(commit_1))?;

        session.call::<_, ()>(id_1, "increment", &(), LIMIT)?;
        session.call::<i16, ()>(id_2, "set", &0x12, LIMIT)?;
        assert_eq!(
            session.call::<_, i64>(id_1, "read_value", &(), LIMIT)?.data,
            0xfe
        );
        assert_eq!(
            session
                .call::<_, Option<i16>>(id_2, "get", &(), LIMIT)?
                .data,
            Some(0x12)
        );
        commit_2 = session.commit()?;
    }

    {
        let vm2 = VM::new(vm.root_dir())?;
        let mut session = vm2.session(SessionData::builder().base(commit_1))?;

        // check if both contracts' state was restored
        assert_eq!(
            session.call::<_, i64>(id_1, "read_value", &(), LIMIT)?.data,
            0xfd
        );
        assert_eq!(
            session
                .call::<_, Option<i16>>(id_2, "get", &(), LIMIT)?
                .data,
            Some(0x11)
        );
    }

    {
        let vm3 = VM::new(vm.root_dir())?;
        let mut session = vm3.session(SessionData::builder().base(commit_2))?;

        // check if both contracts' state was restored
        assert_eq!(
            session.call::<_, i64>(id_1, "read_value", &(), LIMIT)?.data,
            0xfe
        );
        assert_eq!(
            session
                .call::<_, Option<i16>>(id_2, "get", &(), LIMIT)?
                .data,
            Some(0x12)
        );
    }
    Ok(())
}

#[test]
fn contracts_persistence() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;
    let (id_1, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    let (id_2, _) = session.deploy::<_, (), _>(
        contract_bytecode!("box"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    session.call::<_, ()>(id_1, "increment", &(), LIMIT)?;
    session.call::<i16, ()>(id_2, "set", &0x11, LIMIT)?;
    assert_eq!(
        session.call::<_, i64>(id_1, "read_value", &(), LIMIT)?.data,
        0xfd
    );
    assert_eq!(
        session
            .call::<_, Option<i16>>(id_2, "get", &(), LIMIT)?
            .data,
        Some(0x11)
    );

    let commit_1 = session.commit()?;

    let vm2 = VM::new(vm.root_dir())?;
    let mut session2 = vm2.session(SessionData::builder().base(commit_1))?;

    // check if both contracts' state was restored
    assert_eq!(
        session2
            .call::<_, i64>(id_1, "read_value", &(), LIMIT)?
            .data,
        0xfd
    );
    assert_eq!(
        session2
            .call::<_, Option<i16>>(id_2, "get", &(), LIMIT)?
            .data,
        Some(0x11)
    );
    Ok(())
}

#[test]
fn migration() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (contract, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    session.call::<_, ()>(contract, "increment", &(), LIMIT)?;
    session.call::<_, ()>(contract, "increment", &(), LIMIT)?;

    let root = session.commit()?;

    let mut session = vm.session(SessionData::builder().base(root))?;

    session = session.migrate(
        contract,
        contract_bytecode!("double_counter"),
        ContractData::builder(),
        LIMIT,
        |new_contract, session| {
            let old_counter_value = session
                .call::<_, i64>(contract, "read_value", &(), LIMIT)?
                .data;

            let (left_counter_value, _) = session
                .call::<_, (i64, i64)>(new_contract, "read_values", &(), LIMIT)?
                .data;

            let diff = old_counter_value - left_counter_value;
            for _ in 0..diff {
                session.call::<_, ()>(
                    new_contract,
                    "increment_left",
                    &(),
                    LIMIT,
                )?;
            }

            Ok(())
        },
    )?;

    let root = session.commit()?;

    let mut session = vm.session(SessionData::builder().base(root))?;

    let (left_counter, right_counter) = session
        .call::<_, (i64, i64)>(contract, "read_values", &(), LIMIT)?
        .data;

    assert_eq!(left_counter, 0xfe);
    assert_eq!(right_counter, 0xcf);

    Ok(())
}

#[test]
fn migration_new_owner() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    const OWNER: [u8; 33] = [1u8; 33];
    const NEW_OWNER: [u8; 33] = [2u8; 33];

    let (contract, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let root = session.commit()?;

    let mut session = vm.session(SessionData::builder().base(root))?;

    session = session.migrate(
        contract,
        contract_bytecode!("metadata"),
        ContractData::builder().owner(NEW_OWNER),
        LIMIT,
        |_, _| Ok(()),
    )?;

    let owner = session
        .call::<_, [u8; 33]>(contract, "read_owner", &(), LIMIT)?
        .data;

    assert_eq!(owner, NEW_OWNER);

    Ok(())
}

#[test]
fn migration_old_owner() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    const OWNER: [u8; 33] = [1u8; 33];

    let (contract, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let root = session.commit()?;

    let mut session = vm.session(SessionData::builder().base(root))?;

    session = session.migrate(
        contract,
        contract_bytecode!("metadata"),
        ContractData::builder(),
        LIMIT,
        |_, _| Ok(()),
    )?;

    let owner = session
        .call::<_, [u8; 33]>(contract, "read_owner", &(), LIMIT)?
        .data;

    assert_eq!(owner, OWNER);

    Ok(())
}

#[test]
fn migration_self_id_remains_same() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    const OWNER: [u8; 33] = [1u8; 33];

    let (contract_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let root = session.commit()?;

    let mut session = vm.session(SessionData::builder().base(root))?;

    session = session.migrate(
        contract_id,
        contract_bytecode!("metadata"),
        ContractData::builder(),
        LIMIT,
        |_, _| Ok(()),
    )?;

    let new_contract_id = session
        .call::<_, ContractId>(contract_id, "read_id", &(), LIMIT)?
        .data;

    assert_eq!(
        contract_id, new_contract_id,
        "The contract ID as seen by the contract should remain the same"
    );

    Ok(())
}

#[test]
fn migration_to_contract_id_pos_collision_is_rejected() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    // Normal migration replaces bytecode at an existing contract ID. This test
    // covers the edge case where migration targets an absent ID; in that case,
    // replace can effectively introduce a new final contract ID and must not be
    // allowed to introduce a Merkle-position collision with an existing ID.
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
        ContractData::builder().owner(OWNER).contract_id(id_b),
        LIMIT,
    )?;
    let root = session.commit()?;

    let session = vm.session(SessionData::builder().base(root))?;
    let err = session
        .migrate(
            id_a,
            contract_bytecode!("box"),
            ContractData::builder().owner(OWNER),
            LIMIT,
            |_, _| Ok(()),
        )
        .expect_err("migration to colliding contract id should be rejected");

    assert!(matches!(
        err,
        Error::ContractPositionCollision {
            contract_id,
            pos: 0,
        } if contract_id == id_a
    ));

    Ok(())
}

#[test]
fn migration_to_pending_id_pos_collision_is_rejected() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let id_a = ContractId::from_bytes([0; 32]);

    // Both IDs map to the same contract Merkle position: id_a sums to zero,
    // while id_b sums to 1 + u32::MAX, which wraps back to zero.
    let mut id_b_bytes = [0u8; 32];
    id_b_bytes[0..4].copy_from_slice(&1u32.to_le_bytes());
    id_b_bytes[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    let id_b = ContractId::from_bytes(id_b_bytes);

    // The temporary migration deployment must use a distinct non-colliding
    // position so the test reaches replace's pending-collision guard.
    let mut temp_id_bytes = [0u8; 32];
    temp_id_bytes[0..4].copy_from_slice(&2u32.to_le_bytes());
    let temp_id = ContractId::from_bytes(temp_id_bytes);

    assert_ne!(id_a, id_b);
    assert_ne!(id_a, temp_id);
    assert_ne!(id_b, temp_id);

    let mut session = vm.session(SessionData::builder())?;
    session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER).contract_id(id_b),
        LIMIT,
    )?;

    let err = session
        .migrate(
            id_a,
            contract_bytecode!("box"),
            ContractData::builder().owner(OWNER).contract_id(temp_id),
            LIMIT,
            |_, _| Ok(()),
        )
        .expect_err(
            "migration to id colliding with pending contract should fail",
        );

    assert!(matches!(
        err,
        Error::ContractPositionCollision {
            contract_id,
            pos: 0,
        } if contract_id == id_a
    ));

    Ok(())
}

#[test]
fn migration_allows_replacement_contract_at_target_pos() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let id_a = ContractId::from_bytes([0; 32]);

    // The replacement's temporary ID maps to the same contract Merkle position
    // as the final target ID. This is allowed because replace removes the
    // temporary ID before inserting the contract data at the target ID.
    let mut temp_id_bytes = [0u8; 32];
    temp_id_bytes[0..4].copy_from_slice(&1u32.to_le_bytes());
    temp_id_bytes[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    let temp_id = ContractId::from_bytes(temp_id_bytes);

    assert_ne!(id_a, temp_id);

    let session = vm.session(SessionData::builder())?;
    let mut session = session.migrate(
        id_a,
        contract_bytecode!("box"),
        ContractData::builder().owner(OWNER).contract_id(temp_id),
        LIMIT,
        |_, _| Ok(()),
    )?;

    session.call::<i16, ()>(id_a, "set", &0x11, LIMIT)?;
    assert_eq!(
        session
            .call::<_, Option<i16>>(id_a, "get", &(), LIMIT)?
            .data,
        Some(0x11)
    );

    Ok(())
}
