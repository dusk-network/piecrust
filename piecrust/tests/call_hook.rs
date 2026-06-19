// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::sync::{Arc, Mutex};

use piecrust::{
    CallHook, ContractData, Error, SessionData, VM, contract_bytecode,
};
use piecrust_uplink::{ContractError, ContractId};

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;

/// Mirrors `dusk_core::transfer::data::ContractCall`.
#[derive(Debug)]
struct ContractCall {
    contract: ContractId,
    fn_name: String,
    fn_args: Vec<u8>,
    call_stack: Vec<ContractId>,
}

/// Records all inter-contract calls observed by a call hook.
#[derive(Clone)]
struct CallRecorder(Arc<Mutex<Vec<ContractCall>>>);

impl CallRecorder {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(Vec::new())))
    }

    fn hook(&self) -> CallHook {
        let inner = self.0.clone();
        Box::new(move |contract, fn_name, fn_args, call_stack| {
            inner.lock().unwrap().push(ContractCall {
                contract: *contract,
                fn_name: fn_name.to_owned(),
                fn_args: fn_args.to_vec(),
                call_stack: call_stack.iter().map(|id| **id).collect(),
            });
            Ok(())
        })
    }

    fn calls(&self) -> Vec<ContractCall> {
        std::mem::take(&mut self.0.lock().unwrap())
    }
}

#[test]
fn call_hook_observes_inter_contract_call() -> Result<(), Error> {
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

    let recorder = CallRecorder::new();
    session.set_call_hook(recorder.hook());

    // Inter-contract call: callcenter -> counter.read_value
    let value: i64 = session
        .call(center_id, "query_counter", &counter_id, LIMIT)?
        .data;
    assert_eq!(value, 0xfc);

    let calls = recorder.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].contract, counter_id);
    assert_eq!(calls[0].fn_name, "read_value");
    assert_eq!(calls[0].call_stack, vec![center_id]);

    Ok(())
}

#[test]
fn call_hook_not_called_for_direct_calls() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (counter_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let recorder = CallRecorder::new();
    session.set_call_hook(recorder.hook());

    // Direct call from host — should NOT trigger the hook
    let value: i64 = session.call(counter_id, "read_value", &(), LIMIT)?.data;
    assert_eq!(value, 0xfc);

    assert!(recorder.calls().is_empty());

    Ok(())
}

#[test]
fn call_hook_observes_multiple_iccs() -> Result<(), Error> {
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

    let recorder = CallRecorder::new();
    session.set_call_hook(recorder.hook());

    session.call::<_, i64>(center_id, "query_counter", &counter_id, LIMIT)?;
    session.call::<_, ()>(
        center_id,
        "increment_counter",
        &counter_id,
        LIMIT,
    )?;
    session.call::<_, i64>(center_id, "query_counter", &counter_id, LIMIT)?;

    let calls = recorder.calls();
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[0].fn_name, "read_value");
    assert_eq!(calls[1].fn_name, "increment");
    assert_eq!(calls[2].fn_name, "read_value");

    for call in &calls {
        assert_eq!(call.contract, counter_id);
        assert_eq!(call.call_stack, vec![center_id]);
    }

    Ok(())
}

#[test]
fn call_hook_can_deserialize_fn_args() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (center_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let recorder = CallRecorder::new();
    session.set_call_hook(recorder.hook());

    // call_self_n_times(3) triggers a chain of ICCs:
    //   callcenter -> callcenter.call_self_n_times(2)
    //   callcenter -> callcenter.call_self_n_times(1)
    //   callcenter -> callcenter.call_self_n_times(0)
    let _: Vec<ContractId> = session
        .call(center_id, "call_self_n_times", &3u32, LIMIT)?
        .data;

    let calls = recorder.calls();
    assert_eq!(calls.len(), 3);

    for (i, call) in calls.iter().enumerate() {
        assert_eq!(call.contract, center_id);
        assert_eq!(call.fn_name, "call_self_n_times");

        let arg: u32 = rkyv::from_bytes(&call.fn_args)
            .expect("fn_args should deserialize as u32");
        assert_eq!(arg, 2 - i as u32);
        assert_eq!(call.call_stack.len(), i + 1);
        assert!(call.call_stack.iter().all(|id| *id == center_id));
    }

    Ok(())
}

#[test]
fn call_hook_stack_is_immediate_caller_first() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (counter_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    let outer_id = ContractId::from_bytes([0x11; 32]);
    let (outer_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER).contract_id(outer_id),
        LIMIT,
    )?;
    let inner_id = ContractId::from_bytes([0x22; 32]);
    let (inner_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER).contract_id(inner_id),
        LIMIT,
    )?;

    let inner_args = rkyv::to_bytes::<_, 1024>(&(
        counter_id,
        String::from("read_value"),
        Vec::<u8>::new(),
    ))
    .expect("inner args should serialize")
    .to_vec();

    let recorder = CallRecorder::new();
    session.set_call_hook(recorder.hook());

    let res = session
        .call::<_, Result<Vec<u8>, ContractError>>(
            outer_id,
            "delegate_query",
            &(inner_id, String::from("delegate_query"), inner_args),
            LIMIT,
        )?
        .data
        .expect("nested ICC should succeed");
    let inner_res: Result<Vec<u8>, ContractError> =
        rkyv::from_bytes(&res).expect("inner result should decode");
    let value: i64 = rkyv::from_bytes(
        &inner_res.expect("inner counter query should succeed"),
    )
    .expect("counter value should decode");
    assert_eq!(value, 0xfc);

    let calls = recorder.calls();
    assert_eq!(calls.len(), 2);

    assert_eq!(calls[0].contract, inner_id);
    assert_eq!(calls[0].fn_name, "delegate_query");
    assert_eq!(calls[0].call_stack, vec![outer_id]);

    assert_eq!(calls[1].contract, counter_id);
    assert_eq!(calls[1].fn_name, "read_value");
    assert_eq!(calls[1].call_stack, vec![inner_id, outer_id]);

    Ok(())
}

#[test]
fn call_hook_can_reject_call() -> Result<(), Error> {
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

    // Read the initial counter value
    let value: i64 = session.call(counter_id, "read_value", &(), LIMIT)?.data;
    assert_eq!(value, 0xfc);

    // Set a hook that rejects calls to the counter's "increment" function
    let reject_id = counter_id;
    session.set_call_hook(Box::new(move |contract, fn_name, _, _| {
        if *contract == reject_id && fn_name == "increment" {
            Err("increment rejected by test hook".into())
        } else {
            Ok(())
        }
    }));

    // Attempt to increment via callcenter — the hook should reject it
    let result = session.call::<_, ()>(
        center_id,
        "increment_counter",
        &counter_id,
        LIMIT,
    );
    let err = result.expect_err("call should fail when hook rejects");
    let msg = format!("{err}");
    assert!(
        msg.contains("increment rejected by test hook"),
        "error should contain the hook's rejection reason, got: {msg}"
    );

    // Verify the counter value is unchanged
    let value: i64 = session.call(counter_id, "read_value", &(), LIMIT)?.data;
    assert_eq!(value, 0xfc);

    Ok(())
}

#[test]
fn no_hook_set_works_normally() -> Result<(), Error> {
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

    let value: i64 = session
        .call(center_id, "query_counter", &counter_id, LIMIT)?
        .data;
    assert_eq!(value, 0xfc);

    Ok(())
}

#[test]
fn set_and_clear_call_hook_return_previous_hook() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    // No hook set initially — set_call_hook should return None
    let prev = session.set_call_hook(Box::new(|_, _, _, _| Ok(())));
    assert!(prev.is_none(), "first set should return None");

    // Replacing the hook should return the previous one
    let prev =
        session.set_call_hook(Box::new(|_, _, _, _| Err("reject".into())));
    assert!(prev.is_some(), "second set should return the previous hook");

    // Clearing should return the current hook
    let prev = session.clear_call_hook();
    assert!(prev.is_some(), "clear should return the hook");

    // Clearing again should return None
    let prev = session.clear_call_hook();
    assert!(prev.is_none(), "clear on empty should return None");

    Ok(())
}

#[test]
fn clear_call_hook_allows_previously_rejected_call() -> Result<(), Error> {
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

    // Set a hook that rejects all inter-contract calls
    session.set_call_hook(Box::new(|_, _, _, _| Err("rejected".into())));

    // Verify the hook rejects
    let result =
        session.call::<_, i64>(center_id, "query_counter", &counter_id, LIMIT);
    assert!(result.is_err(), "call should fail when hook rejects");

    // Clear the hook
    session.clear_call_hook();

    // The same inter-contract call should now succeed
    let value: i64 = session
        .call(center_id, "query_counter", &counter_id, LIMIT)?
        .data;
    assert_eq!(value, 0xfc);

    Ok(())
}
