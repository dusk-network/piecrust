// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{ContractData, Error, SessionData, VM, contract_bytecode};
use piecrust_uplink::ARGBUF_LEN;

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;
const REPEATED_WRITE_FAILURES: usize = 64;

#[test]
fn counter_read_simple() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let (id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    assert_eq!(
        session.call::<_, i64>(id, "read_value", &(), LIMIT)?.data,
        0xfc
    );

    Ok(())
}

#[test]
fn counter_read_write_simple() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let (id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    assert_eq!(
        session.call::<_, i64>(id, "read_value", &(), LIMIT)?.data,
        0xfc
    );

    session.call::<_, ()>(id, "increment", &(), LIMIT)?;

    assert_eq!(
        session.call::<_, i64>(id, "read_value", &(), LIMIT)?.data,
        0xfd
    );

    Ok(())
}

#[test]
fn call_through_c() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let (counter_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    let (c_example_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("c_example"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    assert_eq!(
        session
            .call::<_, i64>(
                c_example_id,
                "increment_and_read",
                &counter_id,
                LIMIT,
            )?
            .data,
        0xfd
    );

    Ok(())
}

#[test]
fn increment_panic() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let (counter_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("fallible_counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    match session.call::<_, ()>(counter_id, "increment", &true, LIMIT) {
        Err(Error::Panic(panic_msg)) => {
            assert_eq!(panic_msg, String::from("Incremental panic"));
        }
        _ => panic!("Expected a panic error"),
    }

    Ok(())
}

#[test]
fn oversized_argument_failure_cleans_call_context() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let (id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    assert_eq!(
        session.call::<_, i64>(id, "read_value", &(), LIMIT)?.data,
        0xfc
    );

    // Each oversized argument fails after the call context is pushed but before
    // the contract runs. Reversion should prune that context every time,
    // otherwise repeated ARGBUF_LEN + 1 calls eventually exhaust call depth.
    for _ in 0..REPEATED_WRITE_FAILURES {
        let err = session
            .call_raw(id, "increment", vec![0u8; ARGBUF_LEN + 1], LIMIT)
            .expect_err("oversized call argument should be rejected");

        let Error::MemoryAccessOutOfBounds { len, .. } = err else {
            panic!("unexpected error: {err}");
        };
        assert_eq!(len, ARGBUF_LEN + 1);
    }

    session.call::<_, ()>(id, "increment", &(), LIMIT)?;

    assert_eq!(
        session.call::<_, i64>(id, "read_value", &(), LIMIT)?.data,
        0xfd
    );

    Ok(())
}
