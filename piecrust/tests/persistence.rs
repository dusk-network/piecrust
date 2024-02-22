// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{contract_bytecode, ContractData, Error, SessionData, VM};

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
        id_1 = session.deploy(
            contract_bytecode!("counter"),
            ContractData::builder().owner(OWNER),
            LIMIT,
        )?;
        id_2 = session.deploy(
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
    let id_1 = session.deploy(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    let id_2 = session.deploy(
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

    let contract = session.deploy(
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
        ContractData::builder().owner(OWNER),
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
