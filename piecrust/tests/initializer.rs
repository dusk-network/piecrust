// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{
    ContractData, Error, Session, SessionData, VM, contract_bytecode,
};

const CONTRACT_INIT_METHOD: &str = "init";
const GENERATED_MEMORY_INIT_METHOD: &str = "__piecrust_init_memory";
const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;

#[test]
fn init() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let (id, receipt) = session.deploy::<_, (), _>(
        contract_bytecode!("initializer"),
        ContractData::builder().owner(OWNER).init_arg(&0xabu8),
        LIMIT,
    )?;

    let receipt = receipt.expect("deploy with init should return a receipt");
    assert!(receipt.gas_spent > 0, "init should consume gas");
    assert!(
        receipt.gas_spent <= LIMIT,
        "gas_spent must not exceed limit"
    );

    assert_eq!(
        session.call::<_, u8>(id, "read_value", &(), LIMIT)?.data,
        0xab
    );

    // perform transaction and make sure that the contract works as expected
    session.call::<_, ()>(id, "increment", &(), LIMIT)?;
    assert_eq!(
        session.call::<_, u8>(id, "read_value", &(), LIMIT)?.data,
        0xac
    );

    // we should not be able to call init directly
    let result = session.call::<u8, ()>(id, CONTRACT_INIT_METHOD, &0xaa, LIMIT);
    assert!(
        result.is_err(),
        "calling init directly as transaction should not be allowed"
    );

    // make sure the state is still ok
    assert_eq!(
        session.call::<_, u8>(id, "read_value", &(), LIMIT)?.data,
        0xac
    );

    // initialized state should live through across session boundaries
    let commit_id = session.commit()?;
    let mut session = vm.session(SessionData::builder().base(commit_id))?;
    assert_eq!(
        session.call::<_, u8>(id, "read_value", &(), LIMIT)?.data,
        0xac
    );

    // not being able to call init directly should also be enforced across
    // session boundaries
    let result = session.call::<u8, ()>(id, CONTRACT_INIT_METHOD, &0xae, LIMIT);
    assert!(
        result.is_err(),
        "calling init directly should never be allowed"
    );

    Ok(())
}

#[test]
fn generated_memory_init_direct_call_blocked() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let result =
        session.call_raw(id, GENERATED_MEMORY_INIT_METHOD, Vec::new(), LIMIT);
    assert!(
        result.is_err(),
        "calling generated memory initializer directly should not be allowed"
    );

    assert_eq!(
        session.call::<_, i64>(id, "read_value", &(), LIMIT)?.data,
        0xfc
    );

    Ok(())
}

#[test]
fn active_data_not_reapplied_on_reopen() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    // `counter` has no init method: its initial value of 0xfc comes solely from
    // an active data segment that the rewriter moves into the generated memory
    // initializer.
    let (id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    // The active data is applied once, when the persistent memory is first
    // created.
    assert_eq!(
        session.call::<_, i64>(id, "read_value", &(), LIMIT)?.data,
        0xfc
    );

    // Mutate the memory away from its initial active-data value.
    session.call::<_, ()>(id, "increment", &(), LIMIT)?;
    assert_eq!(
        session.call::<_, i64>(id, "read_value", &(), LIMIT)?.data,
        0xfd
    );

    // Persist the mutated memory and reopen it in a fresh session.
    let commit_id = session.commit()?;
    let mut session = vm.session(SessionData::builder().base(commit_id))?;

    // Reopening must not re-run the generated initializer; the mutated value
    // survives instead of being reset to the active-data value of 0xfc.
    assert_eq!(
        session.call::<_, i64>(id, "read_value", &(), LIMIT)?.data,
        0xfd
    );

    Ok(())
}

#[test]
fn init_indirect_call_blocked() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let (empty_initializer_contract_id, receipt) = session.deploy::<_, (), _>(
        contract_bytecode!("empty_initializer"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    assert!(
        receipt.is_some(),
        "empty_initializer has init, should return a receipt"
    );

    let (callcenter_contract_id, receipt) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    assert!(
        receipt.is_none(),
        "callcenter has no init, should return None"
    );

    let result = session.call::<_, ()>(
        callcenter_contract_id,
        "call_init",
        &empty_initializer_contract_id,
        LIMIT,
    );

    assert!(
        result.is_err(),
        "calling init indirectly should not be allowed"
    );

    Ok(())
}

#[test]
fn empty_init_argument() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let (id, receipt) = session.deploy::<_, (), _>(
        contract_bytecode!("empty_initializer"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let receipt = receipt.expect("deploy with init should return a receipt");
    assert!(receipt.gas_spent > 0, "init should consume gas");

    assert_eq!(
        session.call::<_, u8>(id, "read_value", &(), LIMIT)?.data,
        0x10
    );

    Ok(())
}

#[test]
fn deploy_raw_with_init_returns_receipt() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let bytecode = contract_bytecode!("initializer");
    let init_arg = Session::serialize_data(&0xabu8)?;

    let (id, receipt) = session.deploy_raw(
        None,
        bytecode,
        Some(init_arg),
        OWNER.to_vec(),
        LIMIT,
    )?;

    let receipt =
        receipt.expect("deploy_raw with init should return a receipt");
    assert!(receipt.gas_spent > 0, "init should consume gas");
    assert!(
        receipt.gas_spent <= LIMIT,
        "gas_spent must not exceed limit"
    );

    // Verify the init ran
    assert_eq!(
        session.call::<_, u8>(id, "read_value", &(), LIMIT)?.data,
        0xab
    );

    Ok(())
}
