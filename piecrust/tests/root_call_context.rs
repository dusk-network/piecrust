// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{
    ContractData, Error, RootCallContext, SessionData, VM, contract_bytecode,
};
use piecrust_uplink::{ContractError, ContractId};

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;

fn unit_arg() -> Vec<u8> {
    rkyv::to_bytes::<_, 64>(&()).unwrap().to_vec()
}

fn delegated_args(
    contract: ContractId,
    fn_name: &str,
    fn_arg: Vec<u8>,
) -> (ContractId, String, Vec<u8>) {
    (contract, String::from(fn_name), fn_arg)
}

fn decode_delegated(bytes: &[u8]) -> Vec<u8> {
    let result: Result<Vec<u8>, ContractError> =
        rkyv::from_bytes(bytes).unwrap();
    result.expect("delegated call should succeed")
}

#[test]
fn root_context_exposes_only_synthetic_identity() -> Result<(), Error> {
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
    let context = RootCallContext::synthetic_contract(synthetic_caller);

    let receipt = session.call_raw_with_root_context(
        context,
        target,
        "return_caller",
        unit_arg(),
        LIMIT,
    )?;
    let caller: Option<ContractId> = rkyv::from_bytes(&receipt.data).unwrap();
    assert_eq!(caller, Some(synthetic_caller));
    assert_eq!(
        receipt
            .call_tree
            .iter()
            .map(|elem| elem.contract_id)
            .collect::<Vec<_>>(),
        vec![target],
        "the synthetic identity must not become an executable call-tree frame",
    );

    let receipt = session.call_raw_with_root_context(
        context,
        target,
        "return_callstack",
        unit_arg(),
        LIMIT,
    )?;
    let callstack: Vec<ContractId> = rkyv::from_bytes(&receipt.data).unwrap();
    assert_eq!(callstack, vec![synthetic_caller]);

    let caller: Option<ContractId> =
        session.call(target, "return_caller", &(), LIMIT)?.data;
    assert_eq!(caller, None, "ordinary root calls must remain unchanged");

    Ok(())
}

#[test]
fn nested_call_sees_target_then_synthetic_ancestor() -> Result<(), Error> {
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
    let (callee, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder()
            .owner(OWNER)
            .contract_id(ContractId::from_bytes([0x33; 32])),
        LIMIT,
    )?;

    let args = rkyv::to_bytes::<_, 1024>(&(
        callee,
        String::from("return_callstack"),
        unit_arg(),
    ))
    .unwrap()
    .to_vec();
    let receipt = session.call_raw_with_root_context(
        RootCallContext::synthetic_contract(synthetic_caller),
        target,
        "delegate_query",
        args,
        LIMIT,
    )?;
    let result: Result<Vec<u8>, ContractError> =
        rkyv::from_bytes(&receipt.data).unwrap();
    let callstack: Vec<ContractId> =
        rkyv::from_bytes(&result.unwrap()).unwrap();
    assert_eq!(callstack, vec![target, synthetic_caller]);

    let args = rkyv::to_bytes::<_, 1024>(&(
        callee,
        String::from("return_caller"),
        unit_arg(),
    ))
    .unwrap()
    .to_vec();
    let receipt = session.call_raw_with_root_context(
        RootCallContext::synthetic_contract(synthetic_caller),
        target,
        "delegate_query",
        args,
        LIMIT,
    )?;
    let result: Result<Vec<u8>, ContractError> =
        rkyv::from_bytes(&receipt.data).unwrap();
    let caller: Option<ContractId> =
        rkyv::from_bytes(&result.unwrap()).unwrap();
    assert_eq!(caller, Some(target));

    Ok(())
}

#[test]
fn synthetic_ancestry_matches_a_physical_dispatcher_chain() -> Result<(), Error>
{
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;
    let (dispatcher, _) = session.deploy::<_, (), _>(
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
    let (callee, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder()
            .owner(OWNER)
            .contract_id(ContractId::from_bytes([0x33; 32])),
        LIMIT,
    )?;
    let context = RootCallContext::synthetic_contract(dispatcher);

    let physical_target: Result<Vec<u8>, ContractError> = session
        .call(
            dispatcher,
            "delegate_query",
            &delegated_args(target, "return_callstack", unit_arg()),
            LIMIT,
        )?
        .data;
    let physical_target_stack: Vec<ContractId> =
        rkyv::from_bytes(&physical_target.unwrap()).unwrap();
    let synthetic_target = session.call_raw_with_root_context(
        context,
        target,
        "return_callstack",
        unit_arg(),
        LIMIT,
    )?;
    let synthetic_target_stack: Vec<ContractId> =
        rkyv::from_bytes(&synthetic_target.data).unwrap();
    assert_eq!(synthetic_target_stack, physical_target_stack);
    assert_eq!(synthetic_target_stack, vec![dispatcher]);

    let callee_args = delegated_args(callee, "return_callstack", unit_arg());
    let physical_nested: Result<Vec<u8>, ContractError> = session
        .call(
            dispatcher,
            "delegate_query",
            &delegated_args(
                target,
                "delegate_query",
                rkyv::to_bytes::<_, 1024>(&callee_args).unwrap().to_vec(),
            ),
            LIMIT,
        )?
        .data;
    let physical_nested = decode_delegated(&physical_nested.unwrap());
    let physical_nested_stack: Vec<ContractId> =
        rkyv::from_bytes(&physical_nested).unwrap();

    let synthetic_nested = session.call_raw_with_root_context(
        context,
        target,
        "delegate_query",
        rkyv::to_bytes::<_, 1024>(&callee_args).unwrap().to_vec(),
        LIMIT,
    )?;
    let synthetic_nested = decode_delegated(&synthetic_nested.data);
    let synthetic_nested_stack: Vec<ContractId> =
        rkyv::from_bytes(&synthetic_nested).unwrap();

    assert_eq!(synthetic_nested_stack, physical_nested_stack);
    assert_eq!(synthetic_nested_stack, vec![target, dispatcher]);

    Ok(())
}

#[test]
fn callback_into_synthetic_ancestor_matches_physical_abi_identity()
-> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;
    let (dispatcher, _) = session.deploy::<_, (), _>(
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
    let callback_args =
        delegated_args(dispatcher, "return_callstack", unit_arg());

    let physical: Result<Vec<u8>, ContractError> = session
        .call(
            dispatcher,
            "delegate_query",
            &delegated_args(
                target,
                "delegate_query",
                rkyv::to_bytes::<_, 1024>(&callback_args).unwrap().to_vec(),
            ),
            LIMIT,
        )?
        .data;
    let physical = decode_delegated(&physical.unwrap());
    let physical_stack: Vec<ContractId> = rkyv::from_bytes(&physical).unwrap();

    let synthetic = session.call_raw_with_root_context(
        RootCallContext::synthetic_contract(dispatcher),
        target,
        "delegate_query",
        rkyv::to_bytes::<_, 1024>(&callback_args).unwrap().to_vec(),
        LIMIT,
    )?;
    let synthetic = decode_delegated(&synthetic.data);
    let synthetic_stack: Vec<ContractId> =
        rkyv::from_bytes(&synthetic).unwrap();

    assert_eq!(synthetic_stack, physical_stack);
    assert_eq!(synthetic_stack, vec![target, dispatcher]);

    Ok(())
}

#[test]
fn root_context_is_cleared_after_failure() -> Result<(), Error> {
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

    session
        .call_raw_with_root_context(
            RootCallContext::synthetic_contract(synthetic_caller),
            target,
            "panik",
            unit_arg(),
            LIMIT,
        )
        .expect_err("the target should panic");

    let caller: Option<ContractId> =
        session.call(target, "return_caller", &(), LIMIT)?.data;
    assert_eq!(caller, None);

    let error = session
        .call_raw_with_root_context(
            RootCallContext::synthetic_contract(synthetic_caller),
            target,
            "missing_export",
            unit_arg(),
            LIMIT,
        )
        .expect_err("the missing export should fail");
    assert!(
        matches!(error, Error::InvalidFunction(name) if name == "missing_export")
    );

    let caller: Option<ContractId> =
        session.call(target, "return_caller", &(), LIMIT)?.data;
    assert_eq!(caller, None);

    Ok(())
}

#[test]
fn root_context_requires_a_deployed_synthetic_contract() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;
    let (target, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    let missing = ContractId::from_bytes([0xff; 32]);

    let error = session
        .call_raw_with_root_context(
            RootCallContext::synthetic_contract(missing),
            target,
            "return_caller",
            unit_arg(),
            LIMIT,
        )
        .expect_err("a missing synthetic contract must be rejected");
    assert!(matches!(error, Error::ContractDoesNotExist(id) if id == missing));

    Ok(())
}
