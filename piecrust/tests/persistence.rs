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
            ContractData::builder(OWNER),
            LIMIT,
        )?;
        id_2 = session.deploy(
            contract_bytecode!("box"),
            ContractData::builder(OWNER),
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
        ContractData::builder(OWNER),
        LIMIT,
    )?;
    let id_2 = session.deploy(
        contract_bytecode!("box"),
        ContractData::builder(OWNER),
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
