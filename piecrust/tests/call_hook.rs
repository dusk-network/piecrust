// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::sync::{Arc, Mutex};

use piecrust::{
    CallHook, ContractData, Error, Interception, RootCallContext, SessionData,
    VM, contract_bytecode,
};
use piecrust_uplink::{ARGBUF_LEN, ContractError, ContractId, Event};

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;
/// Gas limit for the call-depth tests. Each inter-contract call passes on a
/// share of what is left, so a chain tens of frames deep exhausts `LIMIT`
/// long before it reaches [`MAX_CALL_DEPTH`] — a depth test run on `LIMIT`
/// would pass on gas exhaustion whatever the depth check did.
const DEEP_LIMIT: u64 = 1_000_000_000_000_000;
/// The deepest `call_self_n_times` chain whose final frame still fits under
/// [`MAX_CALL_DEPTH`], with no synthetic ancestor occupying a slot.
const FITTING_DEPTH: u32 = 47;

/// Mirrors `dusk_core::transfer::output::ContractCall`.
#[derive(Debug)]
struct ContractCall {
    caller: ContractId,
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
        Arc::new(move |caller, contract, fn_name, fn_args, ctx| {
            inner.lock().unwrap().push(ContractCall {
                caller: *caller,
                contract: *contract,
                fn_name: fn_name.to_owned(),
                fn_args: fn_args.to_vec(),
                call_stack: ctx.callstack(),
            });
            Ok(None)
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
fn call_hook_observes_synthetic_root_ancestry() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (synthetic_caller, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder()
            .owner(OWNER)
            .contract_id(ContractId::from_bytes([0x11; 32])),
        LIMIT,
    )?;
    let (counter_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    let (center_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder()
            .owner(OWNER)
            .contract_id(ContractId::from_bytes([0x22; 32])),
        LIMIT,
    )?;

    let recorder = CallRecorder::new();
    session.set_call_hook(recorder.hook());

    let args = rkyv::to_bytes::<_, 64>(&counter_id).unwrap().to_vec();
    let receipt = session.call_raw_with_root_context(
        RootCallContext::synthetic_contract(synthetic_caller),
        center_id,
        "query_counter",
        args,
        LIMIT,
    )?;
    let value: i64 = rkyv::from_bytes(&receipt.data).unwrap();
    assert_eq!(value, 0xfc);

    let calls = recorder.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].contract, center_id);
    assert_eq!(calls[0].call_stack, vec![synthetic_caller]);
    assert_eq!(calls[1].contract, counter_id);
    assert_eq!(calls[1].call_stack, vec![center_id, synthetic_caller]);

    Ok(())
}

#[test]
fn rejected_synthetic_root_call_clears_context() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;
    let (synthetic_caller, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder()
            .owner(OWNER)
            .contract_id(ContractId::from_bytes([0x11; 32])),
        LIMIT,
    )?;
    let (target, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder()
            .owner(OWNER)
            .contract_id(ContractId::from_bytes([0x22; 32])),
        LIMIT,
    )?;

    session.set_call_hook(Arc::new(|_, _, _, _, _| {
        Err(ContractError::Panic("rejected".into()))
    }));
    let error = session
        .call_raw_with_root_context(
            RootCallContext::synthetic_contract(synthetic_caller),
            target,
            "return_caller",
            rkyv::to_bytes::<_, 64>(&()).unwrap().to_vec(),
            LIMIT,
        )
        .expect_err("root hook should reject the call");
    assert!(matches!(error, Error::Panic(message) if message == "rejected"));

    session.clear_call_hook();
    let caller: Option<ContractId> =
        session.call(target, "return_caller", &(), LIMIT)?.data;
    assert_eq!(caller, None);

    Ok(())
}

/// A hook rejecting a root-context call maps its `ContractError` onto the
/// host-facing `Error` — every variant, not just the `Panic` the sibling test
/// drives.
#[test]
fn rejected_synthetic_root_call_maps_every_error_variant() -> Result<(), Error>
{
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;
    let (synthetic_caller, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder()
            .owner(OWNER)
            .contract_id(ContractId::from_bytes([0x11; 32])),
        LIMIT,
    )?;
    let (target, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder()
            .owner(OWNER)
            .contract_id(ContractId::from_bytes([0x22; 32])),
        LIMIT,
    )?;

    for rejection in [
        ContractError::OutOfGas,
        ContractError::DoesNotExist,
        ContractError::Unknown,
    ] {
        let expected = rejection.clone();
        session.set_call_hook(Arc::new(move |_, _, _, _, _| {
            Err(expected.clone())
        }));
        let error = session
            .call_raw_with_root_context(
                RootCallContext::synthetic_contract(synthetic_caller),
                target,
                "return_caller",
                rkyv::to_bytes::<_, 64>(&()).unwrap().to_vec(),
                LIMIT,
            )
            .expect_err("the root hook should reject the call");

        match (&rejection, &error) {
            (ContractError::OutOfGas, Error::OutOfGas) => {}
            (
                ContractError::DoesNotExist,
                Error::ContractDoesNotExist(contract),
            ) if *contract == target => {}
            // `Unknown` has no host-side counterpart, so it is surfaced as a
            // panic naming its origin.
            (ContractError::Unknown, Error::Panic(message))
                if message == "unknown call-hook error" => {}
            _ => panic!("{rejection:?} mapped to unexpected error: {error}"),
        }
    }

    Ok(())
}

/// A root-context call has no caller frame to charge an interception against
/// and no caller arg buffer to write its output into, so the hook cannot
/// answer on the callee's behalf: the call is refused rather than run anyway.
#[test]
fn intercepted_synthetic_root_call_is_refused() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;
    let (synthetic_caller, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder()
            .owner(OWNER)
            .contract_id(ContractId::from_bytes([0x11; 32])),
        LIMIT,
    )?;
    let (counter_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    session.set_call_hook(Arc::new(|_, _, _, _, _| {
        Ok(Some(Interception {
            output: Vec::new(),
            gas_spent: 0,
        }))
    }));
    let error = session
        .call_raw_with_root_context(
            RootCallContext::synthetic_contract(synthetic_caller),
            counter_id,
            "increment",
            rkyv::to_bytes::<_, 64>(&()).unwrap().to_vec(),
            LIMIT,
        )
        .expect_err("a root-context interception should be refused");
    assert!(
        matches!(&error, Error::SessionError(message)
            if message.contains("cannot be intercepted")),
        "unexpected error: {error}"
    );

    // The callee must not have run behind the refusal.
    session.clear_call_hook();
    assert_eq!(
        session
            .call::<_, i64>(counter_id, "read_value", &(), LIMIT)?
            .data,
        0xfc
    );

    Ok(())
}

/// A hook firing on a root-context call runs with an empty call tree, so it
/// has no contract to charge a nested call to. `call_as_raw` must report that
/// as an error rather than panicking.
#[test]
fn hook_call_as_raw_without_active_call_errors() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;
    let (synthetic_caller, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder()
            .owner(OWNER)
            .contract_id(ContractId::from_bytes([0x11; 32])),
        LIMIT,
    )?;
    let (counter_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let observed = Arc::new(Mutex::new(None));
    let obs = observed.clone();
    session.set_call_hook(Arc::new(move |_, callee, _, _, ctx| {
        let result = ctx.call_as_raw(*callee, *callee, "read_value", &[], 0);
        *obs.lock().unwrap() =
            Some(result.err().map(|err| err.to_string()).unwrap_or_default());
        Ok(None)
    }));

    session.call_raw_with_root_context(
        RootCallContext::synthetic_contract(synthetic_caller),
        counter_id,
        "read_value",
        rkyv::to_bytes::<_, 64>(&()).unwrap().to_vec(),
        LIMIT,
    )?;

    let observed = observed.lock().unwrap().clone();
    let observed = observed.expect("the hook should have fired");
    assert!(
        observed.contains("call_as_raw requires an active contract call"),
        "unexpected nested call outcome: {observed}"
    );

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

/// A hook can reject a call when a given contract appears *anywhere* above
/// the callee in the call stack, not just as the immediate caller — the
/// pattern used to reject nested state mutations of a protected contract
/// (e.g. `stake -> X -> stake` re-entrancy).
#[test]
fn call_hook_ancestor_check_rejects_nested_call() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let protected_id = ContractId::from_bytes([0x11; 32]);
    let (protected_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder()
            .owner(OWNER)
            .contract_id(protected_id),
        LIMIT,
    )?;
    let proxy_id = ContractId::from_bytes([0x22; 32]);
    let (proxy_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER).contract_id(proxy_id),
        LIMIT,
    )?;

    // Reject any call *to* the protected contract while the protected
    // contract is already somewhere above in the call stack.
    session.set_call_hook(Arc::new(
        move |_caller, contract, _fn_name, _fn_args, ctx| {
            if *contract == protected_id
                && ctx.callstack().contains(&protected_id)
            {
                return Err(ContractError::Panic(
                    "nested call to protected contract".into(),
                ));
            }
            Ok(None)
        },
    ));

    // Allowed: host -> proxy -> protected. The protected contract is the
    // callee but not an ancestor.
    let inner_args = rkyv::to_bytes::<(), 0>(&())
        .expect("args should serialize")
        .to_vec();
    let res = session
        .call::<_, Result<Vec<u8>, ContractError>>(
            proxy_id,
            "delegate_query",
            &(protected_id, String::from("return_self_id"), inner_args),
            LIMIT,
        )?
        .data;
    assert!(res.is_ok(), "call without protected ancestor should pass");

    // Rejected: host -> protected -> proxy -> protected. When the proxy
    // calls back into the protected contract, the protected contract is
    // already an ancestor (protected -> proxy -> protected re-entrancy).
    let reentry_args = rkyv::to_bytes::<_, 1024>(&(
        protected_id,
        String::from("return_self_id"),
        rkyv::to_bytes::<(), 0>(&())
            .expect("args should serialize")
            .to_vec(),
    ))
    .expect("re-entry args should serialize")
    .to_vec();
    let res = session
        .call::<_, Result<Vec<u8>, ContractError>>(
            protected_id,
            "delegate_query",
            &(proxy_id, String::from("delegate_query"), reentry_args),
            LIMIT,
        )?
        .data
        .expect("intermediate ICC should succeed");
    let inner_res: Result<Vec<u8>, ContractError> =
        rkyv::from_bytes(&res).expect("inner result should decode");
    assert_eq!(
        inner_res,
        Err(ContractError::Panic(
            "nested call to protected contract".into()
        )),
        "re-entrant call to the protected contract must be rejected"
    );

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
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, _ctx| {
            if *contract == reject_id && fn_name == "increment" {
                Err(ContractError::Panic(
                    "increment rejected by test hook".into(),
                ))
            } else {
                Ok(None)
            }
        },
    ));

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

/// A hook that rejects with [`ContractError::Unknown`] must surface that exact
/// variant to the calling contract, not a flattened `Panic`. `delegate_query`
/// returns the raw ICC `Result` instead of unwrapping, so the caller observes
/// the hook's `ContractError` verbatim.
#[test]
fn call_hook_surfaces_unknown_error_variant() -> Result<(), Error> {
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

    let reject_id = counter_id;
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, _ctx| {
            if *contract == reject_id && fn_name == "read_value" {
                Err(ContractError::Unknown)
            } else {
                Ok(None)
            }
        },
    ));

    let result = session
        .call::<_, Result<Vec<u8>, ContractError>>(
            center_id,
            "delegate_query",
            &(counter_id, String::from("read_value"), Vec::<u8>::new()),
            LIMIT,
        )?
        .data;
    assert_eq!(
        result,
        Err(ContractError::Unknown),
        "hook's `Unknown` must reach the caller unchanged"
    );

    Ok(())
}

/// The `Panic` variant likewise round-trips through the caller, message
/// included — the counterpart to the `Unknown` case above.
#[test]
fn call_hook_surfaces_panic_error_variant() -> Result<(), Error> {
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

    let reject_id = counter_id;
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, _ctx| {
            if *contract == reject_id && fn_name == "read_value" {
                Err(ContractError::Panic("boom".into()))
            } else {
                Ok(None)
            }
        },
    ));

    let result = session
        .call::<_, Result<Vec<u8>, ContractError>>(
            center_id,
            "delegate_query",
            &(counter_id, String::from("read_value"), Vec::<u8>::new()),
            LIMIT,
        )?
        .data;
    assert_eq!(
        result,
        Err(ContractError::Panic("boom".into())),
        "hook's `Panic` message must reach the caller unchanged"
    );

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
    let prev = session.set_call_hook(Arc::new(|_, _, _, _, _| Ok(None)));
    assert!(prev.is_none(), "first set should return None");

    // Replacing the hook should return the previous one
    let prev = session.set_call_hook(Arc::new(|_, _, _, _, _| {
        Err(ContractError::Panic("reject".into()))
    }));
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
    session.set_call_hook(Arc::new(|_, _, _, _, _| {
        Err(ContractError::Panic("rejected".into()))
    }));

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

#[test]
fn call_hook_can_reroute_call() -> Result<(), Error> {
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

    // Set a hook that intercepts read_value and returns a custom result
    let intercept_id = counter_id;
    let custom_value: i64 = 42;
    let custom_bytes =
        rkyv::to_bytes::<i64, 8>(&custom_value).unwrap().to_vec();
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, _ctx| {
            if *contract == intercept_id && fn_name == "read_value" {
                Ok(Some(Interception {
                    output: custom_bytes.clone(),
                    gas_spent: 0,
                }))
            } else {
                Ok(None)
            }
        },
    ));

    // The callcenter queries counter.read_value, but the hook intercepts it
    let value: i64 = session
        .call(center_id, "query_counter", &counter_id, LIMIT)?
        .data;
    assert_eq!(value, 42, "hook should have rerouted the call");

    // Direct call should NOT be intercepted (hooks only fire on ICC)
    let direct_value: i64 =
        session.call(counter_id, "read_value", &(), LIMIT)?.data;
    assert_eq!(direct_value, 0xfc, "direct call should bypass hook");

    Ok(())
}

#[test]
fn call_hook_reroute_preserves_call_tree() -> Result<(), Error> {
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

    // Hook that reroutes read_value but allows increment through
    let intercept_id = counter_id;
    let custom_value: i64 = 99;
    let custom_bytes =
        rkyv::to_bytes::<i64, 8>(&custom_value).unwrap().to_vec();
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, _ctx| {
            if *contract == intercept_id && fn_name == "read_value" {
                Ok(Some(Interception {
                    output: custom_bytes.clone(),
                    gas_spent: 0,
                }))
            } else {
                Ok(None)
            }
        },
    ));

    // Intercepted call
    let receipt = session.call::<_, i64>(
        center_id,
        "query_counter",
        &counter_id,
        LIMIT,
    )?;
    assert_eq!(receipt.data, 99);
    // The call tree should still record the rerouted call, as a child of the
    // caller and marked as never having executed WASM.
    let elems: Vec<_> = receipt.call_tree.iter().collect();
    assert_eq!(elems.len(), 2, "call tree should have 2 entries");
    let rerouted = elems
        .iter()
        .find(|elem| elem.contract_id == counter_id)
        .expect("the rerouted contract should be in the call tree");
    assert!(
        !rerouted.instance_backed,
        "an intercepted call runs no WASM, so its frame is lightweight"
    );
    assert_eq!(rerouted.spent, 0, "this interception charged no gas");

    // Normal call (increment goes through WASM)
    session.call::<_, ()>(
        center_id,
        "increment_counter",
        &counter_id,
        LIMIT,
    )?;

    // Read via rerouted path still returns our custom value
    let value: i64 = session
        .call(center_id, "query_counter", &counter_id, LIMIT)?
        .data;
    assert_eq!(value, 99);

    Ok(())
}

#[test]
fn call_hook_reroute_multiple_in_sequence() -> Result<(), Error> {
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

    // Hook that intercepts ALL calls to counter (both read_value and
    // increment) with rerouted results
    let intercept_id = counter_id;
    let read_result = rkyv::to_bytes::<i64, 8>(&42i64).unwrap().to_vec();
    let increment_result = rkyv::to_bytes::<(), 0>(&()).unwrap().to_vec();
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, _ctx| {
            if *contract == intercept_id {
                match fn_name {
                    "read_value" => Ok(Some(Interception {
                        output: read_result.clone(),
                        gas_spent: 0,
                    })),
                    "increment" => Ok(Some(Interception {
                        output: increment_result.clone(),
                        gas_spent: 0,
                    })),
                    _ => Ok(None),
                }
            } else {
                Ok(None)
            }
        },
    ));

    // Three sequential top-level calls, each triggering a rerouted ICC
    let v1: i64 = session
        .call(center_id, "query_counter", &counter_id, LIMIT)?
        .data;
    assert_eq!(v1, 42);

    session.call::<_, ()>(
        center_id,
        "increment_counter",
        &counter_id,
        LIMIT,
    )?;

    let v2: i64 = session
        .call(center_id, "query_counter", &counter_id, LIMIT)?
        .data;
    assert_eq!(v2, 42, "second rerouted read should also return 42");

    Ok(())
}

#[test]
fn call_hook_reroute_then_normal_call_preserves_state() -> Result<(), Error> {
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

    // Hook that intercepts only "read_value", lets "increment" through
    let intercept_id = counter_id;
    let fake_read = rkyv::to_bytes::<i64, 8>(&999i64).unwrap().to_vec();
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, _ctx| {
            if *contract == intercept_id && fn_name == "read_value" {
                Ok(Some(Interception {
                    output: fake_read.clone(),
                    gas_spent: 0,
                }))
            } else {
                Ok(None)
            }
        },
    ));

    // Intercepted read
    let v: i64 = session
        .call(center_id, "query_counter", &counter_id, LIMIT)?
        .data;
    assert_eq!(v, 999);

    // Normal increment (goes through WASM) — should not break
    session.call::<_, ()>(
        center_id,
        "increment_counter",
        &counter_id,
        LIMIT,
    )?;

    // Clear hook and verify the real counter was incremented
    session.clear_call_hook();
    let real_value: i64 =
        session.call(counter_id, "read_value", &(), LIMIT)?.data;
    assert_eq!(
        real_value,
        0xfc + 1,
        "real counter should have been incremented"
    );

    Ok(())
}

#[test]
fn call_hook_reroute_recursive_chain() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (center_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    // call_self_n_times(3) makes a chain:
    //   host -> callcenter.call_self_n_times(3)
    //     ICC: callcenter -> callcenter.call_self_n_times(2)
    //       ICC: callcenter -> callcenter.call_self_n_times(1)
    //         ICC: callcenter -> callcenter.call_self_n_times(0)
    //           returns callstack()
    //
    // Intercept the final call (n=0) and return a fake callstack.
    // This verifies the hook fires on each ICC in a recursive chain.
    let recorder = CallRecorder::new();
    let rec_hook = recorder.0.clone();
    let cid = center_id;
    session.set_call_hook(Arc::new(
        move |caller, contract, fn_name, fn_args, ctx| {
            rec_hook.lock().unwrap().push(ContractCall {
                caller: *caller,
                contract: *contract,
                fn_name: fn_name.to_owned(),
                fn_args: fn_args.to_vec(),
                call_stack: ctx.callstack(),
            });
            // Let all calls through normally
            Ok(None)
        },
    ));

    let stack: Vec<ContractId> = session
        .call(center_id, "call_self_n_times", &3u32, LIMIT)?
        .data;

    let calls = recorder.calls();
    // Should have observed 3 ICCs (n=2, n=1, n=0)
    assert_eq!(calls.len(), 3, "hook should fire on each recursive ICC");
    for (i, call) in calls.iter().enumerate() {
        assert_eq!(call.contract, cid);
        assert_eq!(call.fn_name, "call_self_n_times");
        // The stack grows by one caller frame with each recursion level
        assert_eq!(call.call_stack.len(), i + 1);
        assert!(call.call_stack.iter().all(|id| *id == cid));
    }

    // The final call (n=0) returns callstack() which should be
    // [callcenter, callcenter, callcenter] — 3 callers above the
    // deepest frame
    assert_eq!(stack.len(), 3, "callstack at depth 4 should have 3 callers");
    for id in &stack {
        assert_eq!(*id, cid);
    }

    Ok(())
}

#[test]
fn call_hook_reroute_inside_call_as() -> Result<(), Error> {
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

    // Hook that intercepts read_value on counter
    let intercept_id = counter_id;
    let custom_value: i64 = 777;
    let custom_bytes =
        rkyv::to_bytes::<i64, 8>(&custom_value).unwrap().to_vec();
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, _ctx| {
            if *contract == intercept_id && fn_name == "read_value" {
                Ok(Some(Interception {
                    output: custom_bytes.clone(),
                    gas_spent: 0,
                }))
            } else {
                Ok(None)
            }
        },
    ));

    let fake_caller = ContractId::from_bytes([0xAB; 32]);

    // call_as(fake_caller, callcenter, "query_counter", counter_id)
    //
    // Inside callcenter, it makes an ICC to counter.read_value which
    // the hook intercepts. This exercises both call_as and call-hook
    // rerouting in the same call chain.
    let value: i64 = session
        .call_as(fake_caller, center_id, "query_counter", &counter_id, LIMIT)?
        .data;
    assert_eq!(
        value, 777,
        "hook should intercept ICC inside call_as callee"
    );

    // Verify the caller identity was correct during the call
    let caller: Option<ContractId> = session
        .call_as(fake_caller, center_id, "return_caller", &(), LIMIT)?
        .data;
    assert_eq!(caller, Some(fake_caller));

    // Clear hook and verify real counter is untouched
    session.clear_call_hook();
    let real_value: i64 =
        session.call(counter_id, "read_value", &(), LIMIT)?.data;
    assert_eq!(real_value, 0xfc, "real counter should be unchanged");

    Ok(())
}

#[test]
fn call_hook_interception_charges_gas() -> Result<(), Error> {
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

    let intercept_id = counter_id;
    let custom_bytes = rkyv::to_bytes::<i64, 8>(&42i64).unwrap().to_vec();

    // Warm up module cache so subsequent calls have stable gas costs
    let warmup_bytes = custom_bytes.clone();
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, _ctx| {
            if *contract == intercept_id && fn_name == "read_value" {
                Ok(Some(Interception {
                    output: warmup_bytes.clone(),
                    gas_spent: 0,
                }))
            } else {
                Ok(None)
            }
        },
    ));
    session.call::<_, i64>(center_id, "query_counter", &counter_id, LIMIT)?;

    // Measure gas with gas_spent: 0
    let bytes_for_free = custom_bytes.clone();
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, _ctx| {
            if *contract == intercept_id && fn_name == "read_value" {
                Ok(Some(Interception {
                    output: bytes_for_free.clone(),
                    gas_spent: 0,
                }))
            } else {
                Ok(None)
            }
        },
    ));

    let receipt_free = session.call::<_, i64>(
        center_id,
        "query_counter",
        &counter_id,
        LIMIT,
    )?;

    // Now intercept with a non-zero gas charge
    let gas_charge: u64 = 5_000;
    let bytes_for_charged = custom_bytes.clone();
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, _ctx| {
            if *contract == intercept_id && fn_name == "read_value" {
                Ok(Some(Interception {
                    output: bytes_for_charged.clone(),
                    gas_spent: gas_charge,
                }))
            } else {
                Ok(None)
            }
        },
    ));

    let receipt_charged = session.call::<_, i64>(
        center_id,
        "query_counter",
        &counter_id,
        LIMIT,
    )?;

    assert_eq!(receipt_free.data, 42);
    assert_eq!(receipt_charged.data, 42);
    assert_eq!(
        receipt_charged.gas_spent - receipt_free.gas_spent,
        gas_charge,
        "the gas charge from the interception should be reflected in the receipt"
    );

    Ok(())
}

#[test]
fn call_hook_interception_gas_above_callee_limit_is_out_of_gas()
-> Result<(), Error> {
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

    // Intercept with gas_spent far exceeding any possible callee limit.
    // A real callee spending that much would run out of gas, so the
    // interception must surface `OutOfGas` to the caller instead of
    // silently truncating the charge.
    let intercept_id = counter_id;
    let custom_bytes = rkyv::to_bytes::<i64, 8>(&42i64).unwrap().to_vec();
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, _ctx| {
            if *contract == intercept_id && fn_name == "read_value" {
                Ok(Some(Interception {
                    output: custom_bytes.clone(),
                    gas_spent: u64::MAX,
                }))
            } else {
                Ok(None)
            }
        },
    ));

    // delegate_query returns the raw ICC result, so the caller observes
    // the `OutOfGas` directly.
    let result = session
        .call::<_, Result<Vec<u8>, ContractError>>(
            center_id,
            "delegate_query",
            &(counter_id, String::from("read_value"), Vec::<u8>::new()),
            LIMIT,
        )?
        .data;
    assert_eq!(
        result,
        Err(ContractError::OutOfGas),
        "an interception charging more than the callee limit is out of gas"
    );

    Ok(())
}

#[test]
fn call_hook_interception_gas_recorded_in_call_tree() -> Result<(), Error> {
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

    let intercept_id = counter_id;
    let gas_charge: u64 = 10_000;
    let custom_bytes = rkyv::to_bytes::<i64, 8>(&42i64).unwrap().to_vec();
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, _ctx| {
            if *contract == intercept_id && fn_name == "read_value" {
                Ok(Some(Interception {
                    output: custom_bytes.clone(),
                    gas_spent: gas_charge,
                }))
            } else {
                Ok(None)
            }
        },
    ));

    let receipt = session.call::<_, i64>(
        center_id,
        "query_counter",
        &counter_id,
        LIMIT,
    )?;

    // The call tree should contain both the callcenter and the intercepted
    // counter call. The intercepted call's spent should reflect gas_charge.
    let elems: Vec<_> = receipt.call_tree.iter().collect();
    assert_eq!(elems.len(), 2, "call tree should have 2 entries");

    let intercepted = elems
        .iter()
        .find(|e| e.contract_id == counter_id)
        .expect("intercepted contract should be in the call tree");
    assert_eq!(
        intercepted.spent, gas_charge,
        "call tree should record the interception gas charge"
    );
    assert!(
        !intercepted.instance_backed,
        "an intercepted callee is never instantiated"
    );

    let executed = elems
        .iter()
        .find(|e| e.contract_id == center_id)
        .expect("the calling contract should be in the call tree");
    assert!(
        executed.instance_backed,
        "a contract that ran WASM is instance-backed"
    );

    Ok(())
}

#[test]
fn call_hook_receives_correct_caller() -> Result<(), Error> {
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

    // callcenter -> counter.read_value: caller should be callcenter
    let _: i64 = session
        .call(center_id, "query_counter", &counter_id, LIMIT)?
        .data;

    let calls = recorder.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].caller, center_id,
        "caller should be the contract that initiated the ICC"
    );
    assert_eq!(calls[0].contract, counter_id);

    Ok(())
}

#[test]
fn call_hook_receives_correct_caller_in_recursive_chain() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (center_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let recorder = CallRecorder::new();
    session.set_call_hook(recorder.hook());

    // call_self_n_times(2) makes a chain:
    //   host -> callcenter.call_self_n_times(2)
    //     ICC: callcenter -> callcenter.call_self_n_times(1)
    //       ICC: callcenter -> callcenter.call_self_n_times(0)
    let _: Vec<ContractId> = session
        .call(center_id, "call_self_n_times", &2u32, LIMIT)?
        .data;

    let calls = recorder.calls();
    assert_eq!(calls.len(), 2);
    for call in &calls {
        assert_eq!(
            call.caller, center_id,
            "caller should be callcenter in recursive self-calls"
        );
        assert_eq!(call.contract, center_id);
    }

    Ok(())
}

#[test]
fn call_hook_caller_is_callee_not_call_as_origin() -> Result<(), Error> {
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

    let fake_caller = ContractId::from_bytes([0xAB; 32]);

    // call_as(fake_caller, callcenter, "query_counter", counter_id)
    // Inside callcenter, it makes an ICC to counter.read_value.
    // The hook's caller should be callcenter (the actual ICC originator),
    // NOT fake_caller (the call_as identity).
    let _: i64 = session
        .call_as(fake_caller, center_id, "query_counter", &counter_id, LIMIT)?
        .data;

    let calls = recorder.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].caller, center_id,
        "caller in hook should be the ICC originator, not the call_as identity"
    );
    assert_eq!(calls[0].contract, counter_id);
    assert_eq!(
        calls[0].call_stack,
        vec![center_id, fake_caller],
        "the call_as identity frame should appear in the hook's call stack"
    );

    Ok(())
}

#[test]
fn hook_context_call_as_raw_increments_counter() -> Result<(), Error> {
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

    // Hook intercepts "read_value" by first incrementing the counter via
    // ctx.call_as_raw, then returning the new value.
    let ctr_id = counter_id;
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, ctx| {
            if *contract == ctr_id && fn_name == "read_value" {
                // Increment the counter through the hook context
                let inc_args = rkyv::to_bytes::<(), 0>(&()).unwrap().to_vec();
                ctx.call_as_raw(ctr_id, ctr_id, "increment", &inc_args, LIMIT)
                    .map_err(|e| ContractError::Panic(e.to_string()))?;

                // Now read the counter's actual value
                let (data, _gas) = ctx
                    .call_as_raw(ctr_id, ctr_id, "read_value", &inc_args, LIMIT)
                    .map_err(|e| ContractError::Panic(e.to_string()))?;
                Ok(Some(Interception {
                    output: data,
                    gas_spent: 0,
                }))
            } else {
                Ok(None)
            }
        },
    ));

    // Initial counter value is 0xfc (252)
    let value: i64 = session
        .call(center_id, "query_counter", &counter_id, LIMIT)?
        .data;
    // The hook incremented, so we should see 0xfc + 1 = 253
    assert_eq!(value, 0xfc + 1);

    // Clear hook and verify the real counter was incremented
    session.clear_call_hook();
    let real_value: i64 =
        session.call(counter_id, "read_value", &(), LIMIT)?.data;
    assert_eq!(real_value, 0xfc + 1, "counter should have been incremented");

    Ok(())
}

/// `call_as_raw` follows the inter-contract-call gas convention: `0` (or an
/// over-budget limit) resolves to the default share of the intercepted
/// contract's remaining gas, and the held-back reserve lets a nested
/// failure propagate as its own error instead of starving the outer
/// frames into `OutOfGas`.
#[test]
fn hook_context_call_as_raw_gas_follows_icc_convention() -> Result<(), Error> {
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

    // A `gas_limit` of 0 selects the default share of the intercepted
    // contract's remaining gas — the nested call runs instead of
    // immediately running out of gas.
    let ctr_id = counter_id;
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, ctx| {
            if *contract == ctr_id && fn_name == "read_value" {
                let args = rkyv::to_bytes::<(), 0>(&()).unwrap().to_vec();
                let (data, gas) = ctx
                    .call_as_raw(ctr_id, ctr_id, "read_value", &args, 0)
                    .map_err(|e| ContractError::Panic(e.to_string()))?;
                assert!(gas > 0, "the nested call should have spent gas");
                Ok(Some(Interception {
                    output: data,
                    gas_spent: 0,
                }))
            } else {
                Ok(None)
            }
        },
    ));
    let value: i64 = session
        .call(center_id, "query_counter", &counter_id, LIMIT)?
        .data;
    assert_eq!(value, 0xfc);

    // A failed nested call with an over-budget limit charges only the
    // default share, leaving the intercepted contract enough reserve to
    // propagate the failure without itself running out of gas.
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, ctx| {
            if *contract == ctr_id && fn_name == "read_value" {
                let err = ctx
                    .call_as_raw(
                        ctr_id,
                        ctr_id,
                        "no_such_method",
                        &[],
                        u64::MAX,
                    )
                    .expect_err("nested call to a missing export must fail");
                Err(ContractError::Panic(format!("nested failed: {err}")))
            } else {
                Ok(None)
            }
        },
    ));
    let err = session
        .call::<_, i64>(center_id, "query_counter", &counter_id, LIMIT)
        .expect_err("hook rejection must fail the outer call");
    let msg = format!("{err:?}");
    assert!(
        !msg.contains("OutOfGas"),
        "the gas reserve must keep error propagation from running out of \
         gas, got: {msg}"
    );

    Ok(())
}

#[test]
fn hook_context_call_as_raw_error_reverts_state() -> Result<(), Error> {
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

    // Hook that calls a non-existent function, causing an error.
    // The error should be propagated and the call should fail.
    let ctr_id = counter_id;
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, ctx| {
            if *contract == ctr_id && fn_name == "read_value" {
                let result = ctx.call_as_raw(
                    ctr_id,
                    ctr_id,
                    "nonexistent_function",
                    &[],
                    LIMIT,
                );
                match result {
                    Err(e) => Err(ContractError::Panic(format!(
                        "nested call failed: {e}"
                    ))),
                    Ok((data, _)) => Ok(Some(Interception {
                        output: data,
                        gas_spent: 0,
                    })),
                }
            } else {
                Ok(None)
            }
        },
    ));

    // The hook should propagate the error
    let result =
        session.call::<_, i64>(center_id, "query_counter", &counter_id, LIMIT);
    assert!(result.is_err(), "call should fail when nested call fails");

    Ok(())
}

#[test]
fn hook_context_call_as_raw_successful_mutation_reverted_on_later_failure()
-> Result<(), Error> {
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

    // Hook intercepts read_value on counter:
    //   1. Increments counter via call_as_raw (succeeds — counter is now 0xfd)
    //   2. Calls a non-existent function (fails)
    //   3. Returns Err
    //
    // The outer caller (callcenter.query_counter) unwraps the ICC result,
    // so it panics.  The entire call tree is reverted, including the
    // successful increment from step 1.
    let ctr_id = counter_id;
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, ctx| {
            if *contract == ctr_id && fn_name == "read_value" {
                // Step 1: successful mutation
                let inc_args = rkyv::to_bytes::<(), 0>(&()).unwrap().to_vec();
                ctx.call_as_raw(ctr_id, ctr_id, "increment", &inc_args, LIMIT)
                    .map_err(|e| ContractError::Panic(e.to_string()))?;

                // Step 2: failing call
                let result = ctx.call_as_raw(
                    ctr_id,
                    ctr_id,
                    "nonexistent_function",
                    &[],
                    LIMIT,
                );
                match result {
                    Err(e) => Err(ContractError::Panic(format!(
                        "nested call failed: {e}"
                    ))),
                    Ok((data, _)) => Ok(Some(Interception {
                        output: data,
                        gas_spent: 0,
                    })),
                }
            } else {
                Ok(None)
            }
        },
    ));

    // The outer call should fail (hook error → contract error → unwrap panic)
    let result =
        session.call::<_, i64>(center_id, "query_counter", &counter_id, LIMIT);
    assert!(result.is_err(), "call should fail when hook returns Err");

    // Clear the hook and read the counter directly.
    // The increment from step 1 must have been rolled back.
    session.clear_call_hook();
    let value: i64 = session.call(counter_id, "read_value", &(), LIMIT)?.data;
    assert_eq!(
        value, 0xfc,
        "successful mutation should be reverted when the outer call fails"
    );

    Ok(())
}

#[test]
fn hook_context_call_as_raw_reentrant() -> Result<(), Error> {
    use piecrust_uplink::ContractId;

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

    // Hook that intercepts increment on counter, but lets read_value through.
    // When it intercepts increment, it uses ctx.call_as_raw to call
    // callcenter's query_counter (which in turn makes an ICC to
    // counter.read_value). This exercises re-entrancy: the ICC inside
    // ctx.call_as_raw triggers the hook again.
    let ctr_id = counter_id;
    let ctr2_id = center_id;
    let observed_value = std::sync::Arc::new(std::sync::Mutex::new(None));
    let obs = observed_value.clone();
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, ctx| {
            if *contract == ctr_id && fn_name == "increment" {
                // Call callcenter.query_counter, which will ICC to
                // counter.read_value — the hook fires again for that ICC.
                let args =
                    rkyv::to_bytes::<ContractId, 32>(&ctr_id).unwrap().to_vec();
                let (data, _) = ctx
                    .call_as_raw(
                        ctr2_id,
                        ctr2_id,
                        "query_counter",
                        &args,
                        LIMIT,
                    )
                    .map_err(|e| ContractError::Panic(e.to_string()))?;
                let val: i64 = rkyv::from_bytes(&data).unwrap();
                *obs.lock().unwrap() = Some(val);

                // Return empty result for increment
                let output = rkyv::to_bytes::<(), 0>(&()).unwrap().to_vec();
                Ok(Some(Interception {
                    output,
                    gas_spent: 0,
                }))
            } else {
                Ok(None)
            }
        },
    ));

    // callcenter.increment_counter(counter_id) triggers:
    //   ICC: callcenter -> counter.increment (intercepted by hook)
    //     hook calls ctx.call_as_raw(callcenter, callcenter, "query_counter")
    //       ICC: callcenter -> counter.read_value (hook fires again, passes
    // through)
    session.call::<_, ()>(
        center_id,
        "increment_counter",
        &counter_id,
        LIMIT,
    )?;

    let val = observed_value.lock().unwrap().unwrap();
    assert_eq!(
        val, 0xfc,
        "re-entrant hook should have observed counter value"
    );

    Ok(())
}

#[test]
fn hook_context_call_as_raw_gas_flows_to_outer_receipt() -> Result<(), Error> {
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

    // Baseline: intercept read_value without any nested calls
    let ctr_id = counter_id;
    let fake_result = rkyv::to_bytes::<i64, 8>(&42i64).unwrap().to_vec();
    let bytes_noop = fake_result.clone();
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, _ctx| {
            if *contract == ctr_id && fn_name == "read_value" {
                Ok(Some(Interception {
                    output: bytes_noop.clone(),
                    gas_spent: 0,
                }))
            } else {
                Ok(None)
            }
        },
    ));

    // Warm up
    session.call::<_, i64>(center_id, "query_counter", &counter_id, LIMIT)?;

    let baseline = session
        .call::<_, i64>(center_id, "query_counter", &counter_id, LIMIT)?
        .gas_spent;

    // Now intercept with a nested call_as_raw that increments the counter.
    // The nested call's gas is charged to the intercepted caller's fuel
    // meter by the VM, so it reaches the receipt without the hook passing
    // it through `gas_spent`.
    let bytes_with_call = fake_result.clone();
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, ctx| {
            if *contract == ctr_id && fn_name == "read_value" {
                let args = rkyv::to_bytes::<(), 0>(&()).unwrap().to_vec();
                ctx.call_as_raw(ctr_id, ctr_id, "increment", &args, LIMIT)
                    .map_err(|e| ContractError::Panic(e.to_string()))?;
                Ok(Some(Interception {
                    output: bytes_with_call.clone(),
                    gas_spent: 0,
                }))
            } else {
                Ok(None)
            }
        },
    ));

    let with_nested = session
        .call::<_, i64>(center_id, "query_counter", &counter_id, LIMIT)?
        .gas_spent;

    assert!(
        with_nested > baseline,
        "nested call_as_raw should consume additional gas \
         (baseline={baseline}, with_nested={with_nested})"
    );

    Ok(())
}

#[test]
fn hook_context_emit_interleaves_with_contract_events() -> Result<(), Error> {
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

    // Hook intercepts counter.read_value and emits its own event.
    // callcenter.emit_call_emit does:
    //   1. emit("callcenter", 1u32)
    //   2. ICC to counter.read_value  <-- hook fires here, emits hook event
    //   3. emit("callcenter", 2u32)
    //
    // Expected event order: [callcenter/1, hook_event, callcenter/2]
    let intercept_id = counter_id;
    let custom_bytes = rkyv::to_bytes::<i64, 8>(&0xfci64).unwrap().to_vec();
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, ctx| {
            if *contract == intercept_id && fn_name == "read_value" {
                ctx.emit(Event {
                    source: intercept_id,
                    topic: "hook".into(),
                    data: 99u32.to_le_bytes().to_vec(),
                    reverted: false,
                });
                Ok(Some(Interception {
                    output: custom_bytes.clone(),
                    gas_spent: 0,
                }))
            } else {
                Ok(None)
            }
        },
    ));

    let receipt = session.call::<_, ()>(
        center_id,
        "emit_call_emit",
        &counter_id,
        LIMIT,
    )?;

    let events = &receipt.events;
    assert_eq!(events.len(), 3, "should have 3 events");

    // First: callcenter emits ("callcenter", 1u32)
    assert_eq!(events[0].source, center_id);
    assert_eq!(events[0].topic, "callcenter");
    assert_eq!(events[0].data, 1u32.to_le_bytes());

    // Second: hook emits ("hook", 99u32)
    assert_eq!(events[1].source, intercept_id);
    assert_eq!(events[1].topic, "hook");
    assert_eq!(events[1].data, 99u32.to_le_bytes());

    // Third: callcenter emits ("callcenter", 2u32)
    assert_eq!(events[2].source, center_id);
    assert_eq!(events[2].topic, "callcenter");
    assert_eq!(events[2].data, 2u32.to_le_bytes());

    Ok(())
}

#[test]
fn hook_context_emit_after_call_as_raw_with_callee_events() -> Result<(), Error>
{
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (eventer_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("eventer"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    let (center_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    let (counter_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    // Hook intercepts counter.read_value.  It first calls the eventer
    // (which emits a WASM event), then emits its own event via ctx.emit().
    // Events emitted from inside an interception must interleave with the
    // surrounding contract's own events in emission order.
    //
    // callcenter.emit_call_emit does:
    //   1. emit("callcenter", 1)
    //   2. ICC to counter.read_value  <-- hook fires: a. call_as_raw →
    //      eventer.emit_events(1) → emits ("number", 0) b. ctx.emit("hook", 42)
    //   3. emit("callcenter", 2)
    //
    // Expected: [callcenter/1, number/0, hook/42, callcenter/2]
    let evt_id = eventer_id;
    let ctr_id = counter_id;
    let custom_bytes = rkyv::to_bytes::<i64, 8>(&0xfci64).unwrap().to_vec();
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, ctx| {
            if *contract == ctr_id && fn_name == "read_value" {
                // Call eventer which emits a WASM event
                let args = rkyv::to_bytes::<u32, 4>(&1u32).unwrap().to_vec();
                ctx.call_as_raw(ctr_id, evt_id, "emit_events", &args, LIMIT)
                    .map_err(|e| ContractError::Panic(e.to_string()))?;

                // Then emit our own event
                ctx.emit(Event {
                    source: ctr_id,
                    topic: "hook".into(),
                    data: 42u32.to_le_bytes().to_vec(),
                    reverted: false,
                });

                Ok(Some(Interception {
                    output: custom_bytes.clone(),
                    gas_spent: 0,
                }))
            } else {
                Ok(None)
            }
        },
    ));

    let receipt = session.call::<_, ()>(
        center_id,
        "emit_call_emit",
        &counter_id,
        LIMIT,
    )?;

    let events = &receipt.events;
    assert_eq!(events.len(), 4, "should have 4 events");

    // callcenter emits before ICC
    assert_eq!(events[0].source, center_id);
    assert_eq!(events[0].topic, "callcenter");
    assert_eq!(events[0].data, 1u32.to_le_bytes());

    // eventer emits during call_as_raw
    assert_eq!(events[1].source, eventer_id);
    assert_eq!(events[1].topic, "number");
    assert_eq!(events[1].data, 0u32.to_le_bytes());

    // hook emits after call_as_raw
    assert_eq!(events[2].source, counter_id);
    assert_eq!(events[2].topic, "hook");
    assert_eq!(events[2].data, 42u32.to_le_bytes());

    // callcenter emits after ICC
    assert_eq!(events[3].source, center_id);
    assert_eq!(events[3].topic, "callcenter");
    assert_eq!(events[3].data, 2u32.to_le_bytes());

    Ok(())
}

#[test]
fn hook_context_emit_multiple_iccs_event_ordering() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (counter_a, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    let counter_b_id = ContractId::from_bytes([0xBB; 32]);
    let (counter_b, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder()
            .owner(OWNER)
            .contract_id(counter_b_id),
        LIMIT,
    )?;
    let (center_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    // Hook intercepts read_value on both counters, emitting a distinct
    // event for each.
    //
    // callcenter.emit_call_call_emit does:
    //   1. emit("callcenter", 1)
    //   2. ICC to counter_a.read_value  <-- hook emits ("hook_a", 10)
    //   3. emit("callcenter", 2)
    //   4. ICC to counter_b.read_value  <-- hook emits ("hook_b", 20)
    //   5. emit("callcenter", 3)
    //
    // Expected: [cc/1, hook_a/10, cc/2, hook_b/20, cc/3]
    let id_a = counter_a;
    let id_b = counter_b;
    let custom_bytes = rkyv::to_bytes::<i64, 8>(&0xfci64).unwrap().to_vec();
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, ctx| {
            if fn_name == "read_value" {
                if *contract == id_a {
                    ctx.emit(Event {
                        source: id_a,
                        topic: "hook_a".into(),
                        data: 10u32.to_le_bytes().to_vec(),
                        reverted: false,
                    });
                } else if *contract == id_b {
                    ctx.emit(Event {
                        source: id_b,
                        topic: "hook_b".into(),
                        data: 20u32.to_le_bytes().to_vec(),
                        reverted: false,
                    });
                }
                Ok(Some(Interception {
                    output: custom_bytes.clone(),
                    gas_spent: 0,
                }))
            } else {
                Ok(None)
            }
        },
    ));

    let receipt = session.call::<_, ()>(
        center_id,
        "emit_call_call_emit",
        &(counter_a, counter_b),
        LIMIT,
    )?;

    let events = &receipt.events;
    assert_eq!(events.len(), 5, "should have 5 events");

    assert_eq!(events[0].topic, "callcenter");
    assert_eq!(events[0].data, 1u32.to_le_bytes());

    assert_eq!(events[1].topic, "hook_a");
    assert_eq!(events[1].data, 10u32.to_le_bytes());

    assert_eq!(events[2].topic, "callcenter");
    assert_eq!(events[2].data, 2u32.to_le_bytes());

    assert_eq!(events[3].topic, "hook_b");
    assert_eq!(events[3].data, 20u32.to_le_bytes());

    assert_eq!(events[4].topic, "callcenter");
    assert_eq!(events[4].data, 3u32.to_le_bytes());

    Ok(())
}

/// A hook whose nested `call_as_raw` work outweighs the intercepted caller's
/// own execution must not corrupt the call tree's gas accounting: the nested
/// gas is charged to the intercepted caller's fuel meter, keeping every
/// parent's spent gas at least the sum of its children's.
#[test]
fn hook_context_call_as_raw_nested_gas_charged_to_caller() -> Result<(), Error>
{
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

    // Baseline: intercept without nested calls.
    let ctr_id = counter_id;
    let fake_result = rkyv::to_bytes::<i64, 8>(&42i64).unwrap().to_vec();
    let bytes_noop = fake_result.clone();
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, _ctx| {
            if *contract == ctr_id && fn_name == "read_value" {
                Ok(Some(Interception {
                    output: bytes_noop.clone(),
                    gas_spent: 0,
                }))
            } else {
                Ok(None)
            }
        },
    ));
    // Warm up the module cache, then measure.
    session.call::<_, i64>(center_id, "query_counter", &counter_id, LIMIT)?;
    let baseline = session
        .call::<_, i64>(center_id, "query_counter", &counter_id, LIMIT)?
        .gas_spent;

    // Intercept with many nested increments, so the nested gas far exceeds
    // the intercepted caller's direct spend.
    const NESTED_CALLS: u64 = 600;
    let bytes_nested = fake_result.clone();
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, ctx| {
            if *contract == ctr_id && fn_name == "read_value" {
                let args = rkyv::to_bytes::<(), 0>(&()).unwrap().to_vec();
                for _ in 0..NESTED_CALLS {
                    ctx.call_as_raw(ctr_id, ctr_id, "increment", &args, LIMIT)
                        .map_err(|e| ContractError::Panic(e.to_string()))?;
                }
                Ok(Some(Interception {
                    output: bytes_nested.clone(),
                    gas_spent: 0,
                }))
            } else {
                Ok(None)
            }
        },
    ));

    let receipt = session.call::<_, i64>(
        center_id,
        "query_counter",
        &counter_id,
        LIMIT,
    )?;
    assert_eq!(receipt.data, 42);
    assert!(
        receipt.gas_spent > baseline,
        "nested gas should be charged to the caller \
         (baseline={baseline}, with_nested={})",
        receipt.gas_spent
    );
    assert!(receipt.gas_spent <= LIMIT);

    // All the increments were applied.
    session.clear_call_hook();
    let value: i64 = session.call(counter_id, "read_value", &(), LIMIT)?.data;
    assert_eq!(value, 0xfc + NESTED_CALLS as i64);

    Ok(())
}

/// A hook that runs `call_as_raw` into the currently-executing contract and
/// then rejects must leave that contract's fuel meter consistent: the nested
/// work is charged, never clobbered by the nested call's own gas limit.
#[test]
fn hook_context_call_as_raw_into_executing_contract_charges_gas()
-> Result<(), Error> {
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

    let ctr_id = counter_id;

    // Baseline: reject immediately, no nested call.
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, _ctx| {
            if *contract == ctr_id && fn_name == "read_value" {
                Err(ContractError::Panic("rejected".into()))
            } else {
                Ok(None)
            }
        },
    ));
    // delegate_query handles the rejection gracefully, so the outer call
    // completes and yields a receipt.
    let call_args = (counter_id, String::from("read_value"), Vec::<u8>::new());
    session.call::<_, Result<Vec<u8>, ContractError>>(
        center_id,
        "delegate_query",
        &call_args,
        LIMIT,
    )?;
    let baseline = session
        .call::<_, Result<Vec<u8>, ContractError>>(
            center_id,
            "delegate_query",
            &call_args,
            LIMIT,
        )?
        .gas_spent;

    // Now the hook first calls back into the executing contract (the
    // callcenter) with the full gas limit, then rejects. The nested call
    // shares the callcenter's instance, so a fuel clobber would let the
    // caller resume with more gas than it had.
    let cc_id = center_id;
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, ctx| {
            if *contract == ctr_id && fn_name == "read_value" {
                let args = rkyv::to_bytes::<(), 0>(&()).unwrap().to_vec();
                ctx.call_as_raw(ctr_id, cc_id, "return_self_id", &args, LIMIT)
                    .map_err(|e| ContractError::Panic(e.to_string()))?;
                Err(ContractError::Panic("rejected".into()))
            } else {
                Ok(None)
            }
        },
    ));

    let receipt = session.call::<_, Result<Vec<u8>, ContractError>>(
        center_id,
        "delegate_query",
        &call_args,
        LIMIT,
    )?;
    assert_eq!(
        receipt.data,
        Err(ContractError::Panic("rejected".into())),
        "the hook's rejection must reach the caller"
    );
    assert!(receipt.gas_spent <= LIMIT);
    assert!(
        receipt.gas_spent > baseline,
        "the nested call into the executing contract must be charged \
         (baseline={baseline}, with_nested={})",
        receipt.gas_spent
    );

    Ok(())
}

/// Events emitted inside a failed nested `call_as_raw` subtree are marked as
/// reverted, mirroring the inter-contract-call failure path.
#[test]
fn hook_context_call_as_raw_failed_subtree_events_reverted() -> Result<(), Error>
{
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
    let (eventer_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("eventer"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    let (reverter_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("event_reverter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    // The hook intercepts counter.read_value with a nested call to the
    // event reverter, which makes the eventer emit and then panics. The
    // hook swallows the failure and answers the call itself — the outer
    // call succeeds, but the emitted event's subtree failed.
    let ctr_id = counter_id;
    let rev_id = reverter_id;
    let evt_id = eventer_id;
    let custom_bytes = rkyv::to_bytes::<i64, 8>(&0xfci64).unwrap().to_vec();
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, ctx| {
            if *contract == ctr_id && fn_name == "read_value" {
                let args =
                    rkyv::to_bytes::<ContractId, 32>(&evt_id).unwrap().to_vec();
                // A failed nested call charges its full gas limit, so pass
                // a bounded one to leave the intercepted caller enough gas
                // to resume.
                let result = ctx.call_as_raw(
                    ctr_id,
                    rev_id,
                    "emit_then_panic",
                    &args,
                    LIMIT / 10,
                );
                assert!(result.is_err(), "emit_then_panic should fail");
                Ok(Some(Interception {
                    output: custom_bytes.clone(),
                    gas_spent: 0,
                }))
            } else {
                Ok(None)
            }
        },
    ));

    let receipt = session.call::<_, i64>(
        center_id,
        "query_counter",
        &counter_id,
        LIMIT,
    )?;
    assert_eq!(receipt.data, 0xfc);

    let emitted: Vec<_> = receipt
        .events
        .iter()
        .filter(|event| event.source == eventer_id)
        .collect();
    assert_eq!(emitted.len(), 1, "the eventer's event is in the receipt");
    assert!(
        emitted[0].reverted,
        "an event from a failed nested subtree must be marked reverted"
    );

    Ok(())
}

/// Events a hook emits before rejecting a call are marked as reverted, as a
/// failed WASM callee's events would be.
#[test]
fn call_hook_rejecting_hook_events_reverted() -> Result<(), Error> {
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

    let ctr_id = counter_id;
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, ctx| {
            if *contract == ctr_id && fn_name == "read_value" {
                ctx.emit(Event {
                    source: ctr_id,
                    topic: "hook".into(),
                    data: 7u32.to_le_bytes().to_vec(),
                    reverted: false,
                });
                Err(ContractError::Panic("rejected".into()))
            } else {
                Ok(None)
            }
        },
    ));

    // delegate_query handles the rejection, so the outer call yields a
    // receipt containing the hook's event.
    let receipt = session.call::<_, Result<Vec<u8>, ContractError>>(
        center_id,
        "delegate_query",
        &(counter_id, String::from("read_value"), Vec::<u8>::new()),
        LIMIT,
    )?;
    assert_eq!(receipt.data, Err(ContractError::Panic("rejected".into())));

    let hook_events: Vec<_> = receipt
        .events
        .iter()
        .filter(|event| event.topic == "hook")
        .collect();
    assert_eq!(hook_events.len(), 1);
    assert!(
        hook_events[0].reverted,
        "events emitted by a rejecting hook must be marked reverted"
    );

    Ok(())
}

/// `call_as_raw` arguments larger than the argument buffer are rejected
/// instead of writing past it.
#[test]
fn hook_context_call_as_raw_oversized_args_rejected() -> Result<(), Error> {
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

    let ctr_id = counter_id;
    let observed = Arc::new(Mutex::new(None));
    let obs = observed.clone();
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, ctx| {
            if *contract == ctr_id && fn_name == "read_value" {
                let oversized = vec![0u8; ARGBUF_LEN + 1];
                let result = ctx.call_as_raw(
                    ctr_id,
                    ctr_id,
                    "read_value",
                    &oversized,
                    LIMIT,
                );
                *obs.lock().unwrap() = Some(result.map(|_| ()));
                Err(ContractError::Panic("done".into()))
            } else {
                Ok(None)
            }
        },
    ));

    let _ =
        session.call::<_, i64>(center_id, "query_counter", &counter_id, LIMIT);

    let result = observed
        .lock()
        .unwrap()
        .take()
        .expect("hook should have run");
    assert!(
        matches!(result, Err(Error::MemoryAccessOutOfBounds { .. })),
        "oversized args must be rejected, got: {result:?}"
    );

    Ok(())
}

/// A nested callee claiming a return length beyond the argument buffer is
/// rejected instead of driving an out-of-bounds read.
#[test]
fn hook_context_call_as_raw_bogus_ret_len_rejected() -> Result<(), Error> {
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
    let (badreturn_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("badreturn"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let ctr_id = counter_id;
    let bad_id = badreturn_id;
    let observed = Arc::new(Mutex::new(None));
    let obs = observed.clone();
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, ctx| {
            if *contract == ctr_id && fn_name == "read_value" {
                let result =
                    ctx.call_as_raw(ctr_id, bad_id, "huge_ret_len", &[], LIMIT);
                *obs.lock().unwrap() = Some(result.map(|_| ()));
                Err(ContractError::Panic("done".into()))
            } else {
                Ok(None)
            }
        },
    ));

    let _ =
        session.call::<_, i64>(center_id, "query_counter", &counter_id, LIMIT);

    let result = observed
        .lock()
        .unwrap()
        .take()
        .expect("hook should have run");
    assert!(
        matches!(result, Err(Error::MemoryAccessOutOfBounds { .. })),
        "a bogus ret_len must be rejected, got: {result:?}"
    );

    Ok(())
}

/// When a hook makes successful nested `call_as_raw` calls and then rejects,
/// the nested calls' state persists — so their events must stay live. Only
/// the events the hook emitted itself are marked reverted.
#[test]
fn hook_context_call_as_raw_events_survive_hook_rejection() -> Result<(), Error>
{
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
    let (eventer_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("eventer"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    // The hook increments the counter and makes the eventer emit — both
    // successful nested calls — then emits its own event and rejects.
    let ctr_id = counter_id;
    let evt_id = eventer_id;
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, ctx| {
            if *contract == ctr_id && fn_name == "read_value" {
                let unit_args = rkyv::to_bytes::<(), 0>(&()).unwrap().to_vec();
                ctx.call_as_raw(ctr_id, ctr_id, "increment", &unit_args, LIMIT)
                    .map_err(|e| ContractError::Panic(e.to_string()))?;
                let emit_args =
                    rkyv::to_bytes::<u32, 4>(&5u32).unwrap().to_vec();
                ctx.call_as_raw(
                    ctr_id,
                    evt_id,
                    "emit_and_mutate",
                    &emit_args,
                    LIMIT,
                )
                .map_err(|e| ContractError::Panic(e.to_string()))?;
                ctx.emit(Event {
                    source: ctr_id,
                    topic: "hook".into(),
                    data: 9u32.to_le_bytes().to_vec(),
                    reverted: false,
                });
                Err(ContractError::Panic("rejected".into()))
            } else {
                Ok(None)
            }
        },
    ));

    // delegate_query handles the rejection, so the outer call completes.
    let receipt = session.call::<_, Result<Vec<u8>, ContractError>>(
        center_id,
        "delegate_query",
        &(counter_id, String::from("read_value"), Vec::<u8>::new()),
        LIMIT,
    )?;
    assert_eq!(receipt.data, Err(ContractError::Panic("rejected".into())));

    // The nested calls' state is committed...
    session.clear_call_hook();
    let value: i64 = session.call(counter_id, "read_value", &(), LIMIT)?.data;
    assert_eq!(value, 0xfc + 1, "the nested increment persists");
    let eventer_value: u32 =
        session.call(eventer_id, "read_value", &(), LIMIT)?.data;
    assert_eq!(eventer_value, 5, "the nested eventer mutation persists");

    // ...so their events must stay live, while the hook's own event is
    // marked reverted.
    let eventer_events: Vec<_> = receipt
        .events
        .iter()
        .filter(|event| event.source == eventer_id)
        .collect();
    assert_eq!(eventer_events.len(), 1);
    assert!(
        !eventer_events[0].reverted,
        "events of persisted nested calls must stay live"
    );

    let hook_events: Vec<_> = receipt
        .events
        .iter()
        .filter(|event| event.topic == "hook")
        .collect();
    assert_eq!(hook_events.len(), 1);
    assert!(
        hook_events[0].reverted,
        "the rejecting hook's own event must be marked reverted"
    );

    Ok(())
}

/// An interception whose output exceeds the argument buffer fails the call:
/// the caller is charged the callee limit, the error is surfaced, and the
/// hook's own events are marked reverted.
#[test]
fn call_hook_oversized_interception_output_fails_call() -> Result<(), Error> {
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

    let ctr_id = counter_id;
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, ctx| {
            if *contract == ctr_id && fn_name == "read_value" {
                ctx.emit(Event {
                    source: ctr_id,
                    topic: "hook".into(),
                    data: 3u32.to_le_bytes().to_vec(),
                    reverted: false,
                });
                Ok(Some(Interception {
                    output: vec![0u8; ARGBUF_LEN + 1],
                    gas_spent: 0,
                }))
            } else {
                Ok(None)
            }
        },
    ));

    let receipt = session.call::<_, Result<Vec<u8>, ContractError>>(
        center_id,
        "delegate_query",
        &(counter_id, String::from("read_value"), Vec::<u8>::new()),
        LIMIT,
    )?;
    assert!(
        receipt.data.is_err(),
        "an oversized interception output must fail the call"
    );
    assert!(
        receipt.gas_spent > 0,
        "the failed interception must charge gas"
    );

    let hook_events: Vec<_> = receipt
        .events
        .iter()
        .filter(|event| event.topic == "hook")
        .collect();
    assert_eq!(hook_events.len(), 1);
    assert!(
        hook_events[0].reverted,
        "the failed interception's hook events must be marked reverted"
    );

    Ok(())
}

/// An interception cannot fabricate a successful `init` call — the VM
/// rejects `init` as callee before honoring the interception.
#[test]
fn call_hook_cannot_intercept_init() -> Result<(), Error> {
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

    // Intercept everything with a unit return value.
    let unit_output = rkyv::to_bytes::<(), 0>(&()).unwrap().to_vec();
    session.set_call_hook(Arc::new(move |_, _, _, _, _ctx| {
        Ok(Some(Interception {
            output: unit_output.clone(),
            gas_spent: 0,
        }))
    }));

    // callcenter.call_init makes an ICC to the target's "init" and unwraps
    // the result — without the init guard, the intercept-all hook would
    // fabricate a success. The VM must still reject the call.
    let result =
        session.call::<_, ()>(center_id, "call_init", &counter_id, LIMIT);
    assert!(
        result.is_err(),
        "an interception must not fabricate a successful init call"
    );

    Ok(())
}

/// The event checkpoint for a failing callee is taken after the hook ran: a
/// hook that does nested work and then allows the call must not have that
/// work's events swept when the callee later fails.
#[test]
fn hook_nested_work_events_survive_allowed_callee_failure() -> Result<(), Error>
{
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (center_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    let (eventer_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("eventer"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    let (reverter_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("event_reverter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    // On the ICC to the event reverter, the hook makes the eventer emit and
    // mutate via call_as_raw, emits its own event, and allows the call. The
    // allowed callee then makes the eventer emit again and panics.
    let rev_id = reverter_id;
    let evt_id = eventer_id;
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, ctx| {
            if *contract == rev_id && fn_name == "emit_then_panic" {
                let emit_args =
                    rkyv::to_bytes::<u32, 4>(&5u32).unwrap().to_vec();
                ctx.call_as_raw(
                    rev_id,
                    evt_id,
                    "emit_and_mutate",
                    &emit_args,
                    LIMIT,
                )
                .map_err(|e| ContractError::Panic(e.to_string()))?;
                ctx.emit(Event {
                    source: rev_id,
                    topic: "hook".into(),
                    data: 1u32.to_le_bytes().to_vec(),
                    reverted: false,
                });
            }
            Ok(None)
        },
    ));

    // delegate_query handles the callee's failure, so the outer call
    // completes with a receipt.
    let reverter_args = rkyv::to_bytes::<ContractId, 32>(&eventer_id)
        .unwrap()
        .to_vec();
    let receipt = session.call::<_, Result<Vec<u8>, ContractError>>(
        center_id,
        "delegate_query",
        &(reverter_id, String::from("emit_then_panic"), reverter_args),
        LIMIT,
    )?;
    assert!(receipt.data.is_err(), "the allowed callee fails");

    // The hook's nested mutation persists...
    session.clear_call_hook();
    let eventer_value: u32 =
        session.call(eventer_id, "read_value", &(), LIMIT)?.data;
    assert_eq!(eventer_value, 5, "the nested mutation persists");

    // ...its event and the hook's own event stay live, while the failed
    // callee's event is marked reverted.
    let eventer_events: Vec<_> = receipt
        .events
        .iter()
        .filter(|event| event.source == eventer_id)
        .collect();
    assert_eq!(eventer_events.len(), 2);
    assert_eq!(eventer_events[0].data, 5u32.to_le_bytes());
    assert!(
        !eventer_events[0].reverted,
        "the hook's nested call event must stay live"
    );
    assert_eq!(eventer_events[1].data, 42u32.to_le_bytes());
    assert!(
        eventer_events[1].reverted,
        "the failed callee's event must be marked reverted"
    );

    let hook_events: Vec<_> = receipt
        .events
        .iter()
        .filter(|event| event.topic == "hook")
        .collect();
    assert_eq!(hook_events.len(), 1);
    assert!(
        !hook_events[0].reverted,
        "the hook's own event before an allowed call must stay live"
    );

    Ok(())
}

/// A hook's nested `call_as_raw` cannot call `init`.
#[test]
fn hook_context_call_as_raw_rejects_init() -> Result<(), Error> {
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

    let ctr_id = counter_id;
    let observed = Arc::new(Mutex::new(None));
    let obs = observed.clone();
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, ctx| {
            if *contract == ctr_id && fn_name == "read_value" {
                let result =
                    ctx.call_as_raw(ctr_id, ctr_id, "init", &[], LIMIT);
                *obs.lock().unwrap() = Some(result.map(|_| ()));
                Err(ContractError::Panic("done".into()))
            } else {
                Ok(None)
            }
        },
    ));

    let _ =
        session.call::<_, i64>(center_id, "query_counter", &counter_id, LIMIT);

    let result = observed
        .lock()
        .unwrap()
        .take()
        .expect("hook should have run");
    assert!(
        matches!(result, Err(Error::InitalizationError(_))),
        "a nested call to init must be rejected, got: {result:?}"
    );

    Ok(())
}

/// A hook's nested `call_as_raw` at the maximum call depth trips the
/// lightweight frame's depth check instead of overflowing the call tree.
#[test]
fn hook_context_call_as_raw_at_max_depth_is_rejected() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (center_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    // call_self_n_times(48) fills the call tree to the maximum depth of 48.
    // The ICC attempted by the deepest frame fires the hook first; a nested
    // call_as_raw at that point must be rejected by the depth check.
    let cc_id = center_id;
    let observed = Arc::new(Mutex::new(None));
    let obs = observed.clone();
    session.set_call_hook(Arc::new(
        move |_caller, _contract, fn_name, fn_args, ctx| {
            if fn_name == "call_self_n_times" {
                let n: u32 = rkyv::from_bytes(fn_args)
                    .expect("fn_args should deserialize");
                if n == 0 {
                    let unit_args =
                        rkyv::to_bytes::<(), 0>(&()).unwrap().to_vec();
                    let result = ctx.call_as_raw(
                        cc_id,
                        cc_id,
                        "return_self_id",
                        &unit_args,
                        LIMIT,
                    );
                    *obs.lock().unwrap() = Some(result.map(|_| ()));
                }
            }
            Ok(None)
        },
    ));

    // The chain itself fails at the depth limit — only the hook's recorded
    // observation matters here.
    let _ = session.call::<_, Vec<ContractId>>(
        center_id,
        "call_self_n_times",
        &48u32,
        LIMIT,
    );

    let result = observed
        .lock()
        .unwrap()
        .take()
        .expect("hook should have run at the deepest frame");
    assert!(
        matches!(result, Err(Error::SessionError(_))),
        "a nested call at max depth must be rejected, got: {result:?}"
    );

    Ok(())
}

/// An interception at the maximum call depth cannot push its lightweight
/// frame — the error is surfaced to the caller instead of overflowing the
/// call tree.
#[test]
fn call_hook_interception_at_max_depth_is_rejected() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (center_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        DEEP_LIMIT,
    )?;

    // Intercept the deepest ICC of a chain that fills the call tree — the
    // interception's frame push must fail one frame past the limit.
    let output = rkyv::to_bytes::<Vec<ContractId>, 256>(&Vec::new())
        .unwrap()
        .to_vec();
    session.set_call_hook(Arc::new(
        move |_caller, _contract, fn_name, fn_args, _ctx| {
            if fn_name == "call_self_n_times" {
                let n: u32 = rkyv::from_bytes(fn_args)
                    .expect("fn_args should deserialize");
                if n == 0 {
                    return Ok(Some(Interception {
                        output: output.clone(),
                        gas_spent: 0,
                    }));
                }
            }
            Ok(None)
        },
    ));

    // The control that makes this a depth test rather than a gas test: one
    // frame shallower, on the same limit, the chain goes through.
    session
        .call::<_, Vec<ContractId>>(
            center_id,
            "call_self_n_times",
            &FITTING_DEPTH,
            DEEP_LIMIT,
        )
        .expect("the chain one frame shallower must fit");

    let result = session.call::<_, Vec<ContractId>>(
        center_id,
        "call_self_n_times",
        &(FITTING_DEPTH + 1),
        DEEP_LIMIT,
    );
    assert!(
        result.is_err(),
        "an interception at max depth must fail the call"
    );

    Ok(())
}

/// The synthetic ancestor of a root-context call occupies a stack slot, so
/// it counts against the depth limit for an interception's lightweight frame
/// exactly as it does for an instance-backed one: a chain that just fits
/// without a root context must be one too deep with one.
#[test]
fn interception_at_max_depth_counts_the_synthetic_ancestor() -> Result<(), Error>
{
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (synthetic_caller, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder()
            .owner(OWNER)
            .contract_id(ContractId::from_bytes([0x11; 32])),
        DEEP_LIMIT,
    )?;
    let (center_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder()
            .owner(OWNER)
            .contract_id(ContractId::from_bytes([0x22; 32])),
        DEEP_LIMIT,
    )?;

    let output = rkyv::to_bytes::<Vec<ContractId>, 256>(&Vec::new())
        .unwrap()
        .to_vec();
    session.set_call_hook(Arc::new(
        move |_caller, _contract, fn_name, fn_args, _ctx| {
            if fn_name == "call_self_n_times" {
                let n: u32 = rkyv::from_bytes(fn_args)
                    .expect("fn_args should deserialize");
                if n == 0 {
                    return Ok(Some(Interception {
                        output: output.clone(),
                        gas_spent: 0,
                    }));
                }
            }
            Ok(None)
        },
    ));

    session
        .call::<_, Vec<ContractId>>(
            center_id,
            "call_self_n_times",
            &FITTING_DEPTH,
            DEEP_LIMIT,
        )
        .expect("the chain must fit without a synthetic ancestor");

    let result = session.call_raw_with_root_context(
        RootCallContext::synthetic_contract(synthetic_caller),
        center_id,
        "call_self_n_times",
        rkyv::to_bytes::<_, 64>(&FITTING_DEPTH).unwrap().to_vec(),
        DEEP_LIMIT,
    );
    assert!(
        result.is_err(),
        "the synthetic ancestor must cost the chain its last frame"
    );

    Ok(())
}

/// The remaining `ContractError` variants — `DoesNotExist` and `OutOfGas` —
/// also round-trip through a hook rejection unchanged.
#[test]
fn call_hook_surfaces_remaining_error_variants() -> Result<(), Error> {
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

    let call_args = (counter_id, String::from("read_value"), Vec::<u8>::new());
    let reject_id = counter_id;

    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, _ctx| {
            if *contract == reject_id && fn_name == "read_value" {
                Err(ContractError::DoesNotExist)
            } else {
                Ok(None)
            }
        },
    ));
    let receipt = session.call::<_, Result<Vec<u8>, ContractError>>(
        center_id,
        "delegate_query",
        &call_args,
        LIMIT,
    )?;
    assert_eq!(
        receipt.data,
        Err(ContractError::DoesNotExist),
        "the hook's `DoesNotExist` must reach the caller unchanged"
    );
    // A rejection itself charges no gas: the receipt reflects only the
    // caller's own work, far below the callee limit a real failed callee
    // would have charged.
    assert!(
        receipt.gas_spent < LIMIT / 10,
        "a hook rejection must not charge the callee limit, spent: {}",
        receipt.gas_spent
    );

    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, _ctx| {
            if *contract == reject_id && fn_name == "read_value" {
                Err(ContractError::OutOfGas)
            } else {
                Ok(None)
            }
        },
    ));
    let result = session
        .call::<_, Result<Vec<u8>, ContractError>>(
            center_id,
            "delegate_query",
            &call_args,
            LIMIT,
        )?
        .data;
    assert_eq!(
        result,
        Err(ContractError::OutOfGas),
        "the hook's `OutOfGas` must reach the caller unchanged"
    );

    Ok(())
}

/// A hook rejecting with a panic message larger than the argument buffer
/// must not panic the host — the message is truncated to fit.
#[test]
fn call_hook_oversized_rejection_message_is_truncated() -> Result<(), Error> {
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

    let reject_id = counter_id;
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, _ctx| {
            if *contract == reject_id && fn_name == "read_value" {
                Err(ContractError::Panic("x".repeat(ARGBUF_LEN * 2)))
            } else {
                Ok(None)
            }
        },
    ));

    // Without the truncation, `to_parts` panics the host process while
    // writing the message; with it, the rejection surfaces as an ordinary
    // failed call. (The truncated message still exceeds what the contract
    // can re-serialize alongside its own overhead, so the call errors —
    // gracefully.)
    let result = session.call::<_, Result<Vec<u8>, ContractError>>(
        center_id,
        "delegate_query",
        &(counter_id, String::from("read_value"), Vec::<u8>::new()),
        LIMIT,
    );
    assert!(
        result.is_err(),
        "the oversized rejection must fail gracefully, got: {result:?}"
    );

    // A rejection that fits after truncation round-trips normally.
    let reject_id = counter_id;
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, _ctx| {
            if *contract == reject_id && fn_name == "read_value" {
                Err(ContractError::Panic("y".repeat(ARGBUF_LEN / 2)))
            } else {
                Ok(None)
            }
        },
    ));
    let result = session
        .call::<_, Result<Vec<u8>, ContractError>>(
            center_id,
            "delegate_query",
            &(counter_id, String::from("read_value"), Vec::<u8>::new()),
            LIMIT,
        )?
        .data;
    assert_eq!(
        result,
        Err(ContractError::Panic("y".repeat(ARGBUF_LEN / 2))),
        "a rejection message that fits must reach the caller unchanged"
    );

    Ok(())
}

/// The truncation point of an oversized rejection message may land inside a
/// multi-byte character. Slicing there would panic the host process, so the
/// cut has to walk back to a character boundary.
#[test]
fn call_hook_oversized_rejection_message_cut_inside_a_char() -> Result<(), Error>
{
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

    // One ASCII byte followed by two-byte characters puts every character
    // boundary at an odd index, while the cut at `ARGBUF_LEN - 4` is even.
    let message = format!("x{}", "é".repeat(ARGBUF_LEN));
    assert!(!message.is_char_boundary(ARGBUF_LEN - 4));

    let reject_id = counter_id;
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, _ctx| {
            if *contract == reject_id && fn_name == "read_value" {
                Err(ContractError::Panic(message.clone()))
            } else {
                Ok(None)
            }
        },
    ));

    let result = session.call::<_, Result<Vec<u8>, ContractError>>(
        center_id,
        "delegate_query",
        &(counter_id, String::from("read_value"), Vec::<u8>::new()),
        LIMIT,
    );
    assert!(
        result.is_err(),
        "the oversized rejection must fail gracefully, got: {result:?}"
    );

    Ok(())
}

/// A hook's nested `call_as_raw` to a contract that does not exist fails
/// cleanly: the frames are unwound, the full nested limit is charged, and
/// the session stays usable.
#[test]
fn hook_context_call_as_raw_nonexistent_callee_cleans_up() -> Result<(), Error>
{
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

    let ctr_id = counter_id;
    let missing_id = ContractId::from_bytes([0xEE; 32]);
    let observed = Arc::new(Mutex::new(None));
    let obs = observed.clone();
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, ctx| {
            if *contract == ctr_id && fn_name == "read_value" {
                // Bound the limit so the failed nested call does not drain
                // the intercepted caller.
                let result = ctx.call_as_raw(
                    ctr_id,
                    missing_id,
                    "read_value",
                    &[],
                    LIMIT / 10,
                );
                *obs.lock().unwrap() = Some(result.map(|_| ()));
            }
            Ok(None)
        },
    ));

    // The call itself proceeds normally after the hook's failed nested
    // attempt.
    let value: i64 = session
        .call(center_id, "query_counter", &counter_id, LIMIT)?
        .data;
    assert_eq!(value, 0xfc);

    let result = observed
        .lock()
        .unwrap()
        .take()
        .expect("hook should have run");
    assert!(
        matches!(result, Err(Error::ContractDoesNotExist(_))),
        "a nested call to a missing contract must fail, got: {result:?}"
    );

    Ok(())
}

/// A failed nested `call_as_raw` into the currently-executing contract (shared
/// instance) reverts only the nested call and leaves the suspended caller's
/// execution intact.
#[test]
fn hook_context_call_as_raw_into_executing_contract_failure_is_scoped()
-> Result<(), Error> {
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

    // The hook calls back into the executing callcenter with a function
    // that panics — the nested failure must roll back only the nested
    // call's state, not the suspended callcenter's mid-execution state.
    let ctr_id = counter_id;
    let cc_id = center_id;
    let custom_bytes = rkyv::to_bytes::<i64, 8>(&42i64).unwrap().to_vec();
    let observed = Arc::new(Mutex::new(None));
    let obs = observed.clone();
    session.set_call_hook(Arc::new(
        move |_caller, contract, fn_name, _, ctx| {
            if *contract == ctr_id && fn_name == "read_value" {
                let unit_args = rkyv::to_bytes::<(), 0>(&()).unwrap().to_vec();
                let result = ctx.call_as_raw(
                    cc_id,
                    cc_id,
                    "panik",
                    &unit_args,
                    LIMIT / 10,
                );
                *obs.lock().unwrap() = Some(result.map(|_| ()));
                Ok(Some(Interception {
                    output: custom_bytes.clone(),
                    gas_spent: 0,
                }))
            } else {
                Ok(None)
            }
        },
    ));

    // The suspended callcenter resumes after the failed nested call into
    // its own instance and completes normally.
    let receipt = session.call::<_, i64>(
        center_id,
        "query_counter",
        &counter_id,
        LIMIT,
    )?;
    assert_eq!(receipt.data, 42, "the interception result is returned");
    assert!(receipt.gas_spent <= LIMIT);

    let result = observed
        .lock()
        .unwrap()
        .take()
        .expect("hook should have run");
    assert!(
        matches!(result, Err(Error::Panic(_))),
        "the nested panik must fail, got: {result:?}"
    );

    // The session and both contracts remain fully usable.
    session.clear_call_hook();
    let value: i64 = session
        .call(center_id, "query_counter", &counter_id, LIMIT)?
        .data;
    assert_eq!(value, 0xfc, "counter state is untouched");

    Ok(())
}
