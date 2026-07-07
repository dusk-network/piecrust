// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{ContractData, Error, SessionData, VM, contract_bytecode};
use piecrust_uplink::ContractId;

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;

#[test]
fn call_as_sets_caller_identity() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (center_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    // Create a fake caller ID
    let fake_caller = ContractId::from_bytes([0xAB; 32]);

    // Use call_as so that callcenter sees fake_caller as its caller
    let caller: Option<ContractId> = session
        .call_as(fake_caller, center_id, "return_caller", &(), LIMIT)?
        .data;

    assert_eq!(
        caller,
        Some(fake_caller),
        "callee should see the specified caller"
    );

    Ok(())
}

#[test]
fn call_as_produces_correct_callstack_depth() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (center_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let fake_caller = ContractId::from_bytes([0xAB; 32]);

    // call_as(A, B) should produce stack [A, B] — callstack returns
    // everything above the current contract, so it should be [A]
    let stack: Vec<ContractId> = session
        .call_as(fake_caller, center_id, "return_callstack", &(), LIMIT)?
        .data;

    assert_eq!(stack.len(), 1, "callstack should have depth 1 (the caller)");
    assert_eq!(
        stack[0], fake_caller,
        "callstack should contain the fake caller"
    );

    Ok(())
}

#[test]
fn call_as_with_real_contract_as_caller() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (counter_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    let (center_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    // Use counter as the caller identity when calling callcenter
    let caller: Option<ContractId> = session
        .call_as(counter_id, center_id, "return_caller", &(), LIMIT)?
        .data;

    assert_eq!(
        caller,
        Some(counter_id),
        "callee should see counter as its caller"
    );

    Ok(())
}

#[test]
fn call_as_raw_works() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (counter_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let fake_caller = ContractId::from_bytes([0xCD; 32]);

    // call_as_raw with serialized () argument
    let arg = rkyv::to_bytes::<(), 0>(&()).unwrap().to_vec();
    let receipt = session.call_as_raw(
        fake_caller,
        counter_id,
        "read_value",
        arg,
        LIMIT,
    )?;

    // Deserialize the result as i64
    let value: i64 = rkyv::from_bytes(&receipt.data).unwrap();
    assert_eq!(value, 0xfc);
    assert!(receipt.gas_spent > 0);

    Ok(())
}

#[test]
fn call_as_returns_receipt_with_events_and_gas() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (counter_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let fake_caller = ContractId::from_bytes([0xEF; 32]);

    let receipt = session.call_as::<_, i64>(
        fake_caller,
        counter_id,
        "read_value",
        &(),
        LIMIT,
    )?;

    assert_eq!(receipt.data, 0xfc);
    assert!(receipt.gas_spent > 0);
    assert_eq!(receipt.gas_limit, LIMIT);

    Ok(())
}

#[test]
fn call_as_sequential_calls_clean_up_state() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (center_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let caller_a = ContractId::from_bytes([0xAA; 32]);
    let caller_b = ContractId::from_bytes([0xBB; 32]);

    // First call_as with caller A
    let caller: Option<ContractId> = session
        .call_as(caller_a, center_id, "return_caller", &(), LIMIT)?
        .data;
    assert_eq!(caller, Some(caller_a));

    // Second call_as with caller B — state from first call must be
    // fully cleaned up
    let caller: Option<ContractId> = session
        .call_as(caller_b, center_id, "return_caller", &(), LIMIT)?
        .data;
    assert_eq!(
        caller,
        Some(caller_b),
        "second call_as should see caller B, not A"
    );

    // Normal call after call_as — should have no caller
    let caller: Option<ContractId> =
        session.call(center_id, "return_caller", &(), LIMIT)?.data;
    assert_eq!(
        caller, None,
        "normal call after call_as should have no caller"
    );

    Ok(())
}

#[test]
fn call_as_callee_makes_icc_with_correct_callstack() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (counter_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    let (center_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let fake_caller = ContractId::from_bytes([0xAB; 32]);

    // call_as(fake_caller, callcenter, "query_counter", counter_id)
    //
    // This makes callcenter execute with fake_caller as its caller.
    // Inside, callcenter calls counter.read_value (an ICC).
    // The full stack during counter's execution should be:
    //   [fake_caller, callcenter, counter]
    //
    // We verify the value comes back correctly, proving the ICC
    // inside the callee works with the extra caller frame.
    let value: i64 = session
        .call_as(fake_caller, center_id, "query_counter", &counter_id, LIMIT)?
        .data;
    assert_eq!(value, 0xfc, "ICC inside call_as callee should work");

    Ok(())
}

#[test]
fn call_as_callee_sees_correct_callstack_with_icc() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (center_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let fake_caller = ContractId::from_bytes([0xAB; 32]);

    // call_as(fake_caller, callcenter, "return_callstack")
    // Stack during callcenter: [fake_caller, callcenter]
    // callcenter.return_callstack() returns everything above itself
    // so it should return [fake_caller]
    let stack: Vec<ContractId> = session
        .call_as(fake_caller, center_id, "return_callstack", &(), LIMIT)?
        .data;
    assert_eq!(stack.len(), 1);
    assert_eq!(stack[0], fake_caller);

    // Now call_self_n_times(1) via call_as:
    //   host -> call_as(fake_caller, callcenter)
    //     callcenter.call_self_n_times(1)
    //       ICC: callcenter -> callcenter.call_self_n_times(0)
    //         returns callstack()
    //
    // At the deepest level, callstack should be:
    //   [fake_caller, callcenter, callcenter]
    // callstack() returns everything above current, so:
    //   [fake_caller, callcenter]
    let stack: Vec<ContractId> = session
        .call_as(fake_caller, center_id, "call_self_n_times", &1u32, LIMIT)?
        .data;
    assert_eq!(
        stack.len(),
        2,
        "depth should be 2: callcenter + fake_caller"
    );
    // call_ids order: nearest caller first, root last
    assert_eq!(stack[0], center_id);
    assert_eq!(stack[1], fake_caller);

    Ok(())
}

#[test]
fn call_as_interleaved_with_normal_calls() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (counter_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let fake_caller = ContractId::from_bytes([0xCC; 32]);

    // Normal call
    let v: i64 = session.call(counter_id, "read_value", &(), LIMIT)?.data;
    assert_eq!(v, 0xfc);

    // call_as
    let v: i64 = session
        .call_as(fake_caller, counter_id, "read_value", &(), LIMIT)?
        .data;
    assert_eq!(v, 0xfc);

    // Normal call — increment
    session.call::<_, ()>(counter_id, "increment", &(), LIMIT)?;

    // call_as — should see incremented value
    let v: i64 = session
        .call_as(fake_caller, counter_id, "read_value", &(), LIMIT)?
        .data;
    assert_eq!(v, 0xfc + 1, "call_as should see state from normal call");

    // Normal call — confirm
    let v: i64 = session.call(counter_id, "read_value", &(), LIMIT)?.data;
    assert_eq!(v, 0xfc + 1);

    Ok(())
}

#[test]
fn call_as_self_caller_eq_contract() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (center_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    // call_as where caller == contract — the callee should see itself as
    // its own caller.
    let caller: Option<ContractId> = session
        .call_as(center_id, center_id, "return_caller", &(), LIMIT)?
        .data;
    assert_eq!(
        caller,
        Some(center_id),
        "callee should see itself as caller"
    );

    let stack: Vec<ContractId> = session
        .call_as(center_id, center_id, "return_callstack", &(), LIMIT)?
        .data;
    assert_eq!(stack.len(), 1);
    assert_eq!(stack[0], center_id);

    // Verify state is not corrupted — a normal call afterwards should work
    let caller: Option<ContractId> =
        session.call(center_id, "return_caller", &(), LIMIT)?.data;
    assert_eq!(caller, None, "normal call should have no caller");

    Ok(())
}

#[test]
fn call_as_self_caller_eq_contract_with_state_mutation() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (counter_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    // call_as(counter, counter, "increment") — caller == contract, with
    // a state mutation to exercise the apply path on a duplicated contract
    // ID in the call tree.
    session.call_as::<_, ()>(
        counter_id,
        counter_id,
        "increment",
        &(),
        LIMIT,
    )?;

    let v: i64 = session.call(counter_id, "read_value", &(), LIMIT)?.data;
    assert_eq!(v, 0xfc + 1, "increment should have been applied");

    // Second round — verify the session is still consistent
    session.call_as::<_, ()>(
        counter_id,
        counter_id,
        "increment",
        &(),
        LIMIT,
    )?;

    let v: i64 = session.call(counter_id, "read_value", &(), LIMIT)?.data;
    assert_eq!(v, 0xfc + 2, "second increment should also apply cleanly");

    Ok(())
}

#[test]
fn call_as_reverts_on_panic() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (counter_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("fallible_counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let fake_caller = ContractId::from_bytes([0xAB; 32]);

    // Increment normally via call_as — should succeed
    session.call_as::<_, ()>(
        fake_caller,
        counter_id,
        "increment",
        &false,
        LIMIT,
    )?;
    let v: i64 = session.call(counter_id, "read_value", &(), LIMIT)?.data;
    assert_eq!(v, 0xfd, "value should be incremented");

    // Trigger a panic inside call_as — should revert the call
    let err = session
        .call_as::<_, ()>(fake_caller, counter_id, "increment", &true, LIMIT)
        .unwrap_err();
    assert!(
        matches!(err, Error::Panic(_)),
        "should propagate panic: {err:?}"
    );

    // State should be unchanged after the reverted call
    let v: i64 = session.call(counter_id, "read_value", &(), LIMIT)?.data;
    assert_eq!(v, 0xfd, "value should not change after reverted call_as");

    // Session should still be usable
    session.call_as::<_, ()>(
        fake_caller,
        counter_id,
        "increment",
        &false,
        LIMIT,
    )?;
    let v: i64 = session.call(counter_id, "read_value", &(), LIMIT)?.data;
    assert_eq!(v, 0xfe, "session should remain consistent after revert");

    Ok(())
}

#[test]
fn call_as_self_caller_eq_contract_reverts_on_panic() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (counter_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("fallible_counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    // Increment to establish a known state
    session.call_as::<_, ()>(
        counter_id,
        counter_id,
        "increment",
        &false,
        LIMIT,
    )?;
    let v: i64 = session.call(counter_id, "read_value", &(), LIMIT)?.data;
    assert_eq!(v, 0xfd);

    // call_as(counter, counter, "increment", true) — caller == contract,
    // panics inside the callee. The revert path must handle the lightweight
    // caller frame sharing the same contract_id as the real frame.
    let err = session
        .call_as::<_, ()>(counter_id, counter_id, "increment", &true, LIMIT)
        .unwrap_err();
    assert!(matches!(err, Error::Panic(_)));

    // State should be unchanged
    let v: i64 = session.call(counter_id, "read_value", &(), LIMIT)?.data;
    assert_eq!(
        v, 0xfd,
        "revert with caller==contract should not corrupt state"
    );

    // Session should remain functional
    session.call::<_, ()>(counter_id, "increment", &false, LIMIT)?;
    let v: i64 = session.call(counter_id, "read_value", &(), LIMIT)?.data;
    assert_eq!(v, 0xfe);

    Ok(())
}

#[test]
fn call_as_rejects_init() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (counter_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let fake_caller = ContractId::from_bytes([0xAB; 32]);

    let result =
        session.call_as::<_, ()>(fake_caller, counter_id, "init", &(), LIMIT);
    assert!(
        matches!(result, Err(Error::InitalizationError(_))),
        "call_as to init must be rejected, got: {result:?}"
    );

    let result =
        session.call_as_raw(fake_caller, counter_id, "init", Vec::new(), LIMIT);
    assert!(
        matches!(result, Err(Error::InitalizationError(_))),
        "call_as_raw to init must be rejected, got: {result:?}"
    );

    Ok(())
}

/// Oversized arguments are rejected before any call-tree frame is pushed —
/// the session stays clean and later calls see no phantom caller.
#[test]
fn oversized_args_rejected_without_frame_leak() -> Result<(), Error> {
    use piecrust_uplink::ARGBUF_LEN;

    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (center_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let oversized = vec![0u8; ARGBUF_LEN + 1];

    let result =
        session.call_raw(center_id, "return_caller", oversized.clone(), LIMIT);
    assert!(
        matches!(result, Err(Error::MemoryAccessOutOfBounds { .. })),
        "oversized args to call_raw must be rejected, got: {result:?}"
    );

    let fake_caller = ContractId::from_bytes([0xAB; 32]);
    let result = session.call_as_raw(
        fake_caller,
        center_id,
        "return_caller",
        oversized,
        LIMIT,
    );
    assert!(
        matches!(result, Err(Error::MemoryAccessOutOfBounds { .. })),
        "oversized args to call_as_raw must be rejected, got: {result:?}"
    );

    // No frame leaked: a subsequent call sees no phantom caller.
    let caller: Option<ContractId> =
        session.call(center_id, "return_caller", &(), LIMIT)?.data;
    assert_eq!(caller, None, "no caller frame may leak from failed calls");

    Ok(())
}

/// A `call_as` to a contract that does not exist fails cleanly: the caller
/// identity frame is unwound and the session stays usable.
#[test]
fn call_as_nonexistent_callee_cleans_up() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (center_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let fake_caller = ContractId::from_bytes([0xAB; 32]);
    let missing_id = ContractId::from_bytes([0xEE; 32]);

    let result = session.call_as::<_, ()>(
        fake_caller,
        missing_id,
        "read_value",
        &(),
        LIMIT,
    );
    assert!(
        matches!(result, Err(Error::ContractDoesNotExist(_))),
        "a call_as to a missing contract must fail, got: {result:?}"
    );

    // The session stays clean: no phantom caller leaks into later calls.
    let caller: Option<ContractId> =
        session.call(center_id, "return_caller", &(), LIMIT)?.data;
    assert_eq!(
        caller, None,
        "no caller frame may leak from the failed call"
    );

    Ok(())
}
