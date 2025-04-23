// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{
    contract_bytecode, gen_contract_id, ContractData, ContractError,
    ContractId, Error, SessionData, VM,
};

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;
const CONTRACT_DEPLOY_CONTRACT_LIMIT: u64 = 7_500_000;
const GAS_PER_DEPLOY_BYTE: u64 = 100;
const CONTRACT_DEPLOYER_TEMPLATE_CODE: &[u8] = include_bytes!("../../target/wasm64-unknown-unknown/release/counter_deployer_template.wasm");
const EXPECTED_DEPLOY_CHARGE: u64 =
    CONTRACT_DEPLOYER_TEMPLATE_CODE.len() as u64 * GAS_PER_DEPLOY_BYTE;

#[test]
pub fn deploy_with_id() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let bytecode = contract_bytecode!("counter");
    let some_id = [1u8; 32];
    let contract_id = ContractId::from(some_id);
    let mut session = vm.session(SessionData::builder())?;
    session.deploy(
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
    session.deploy(
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
pub fn contract_deploy_contract_simple() -> Result<(), Error> {
    let vm =
        VM::ephemeral_with_session_config(Some(GAS_PER_DEPLOY_BYTE), None)?;

    let bytecode = contract_bytecode!("counter_deployer");
    let contract_id = ContractId::from([1; 32]);
    let mut session = vm.session(SessionData::builder())?;
    session.deploy(
        bytecode,
        ContractData::builder()
            .owner(OWNER)
            .contract_id(contract_id),
        LIMIT,
    )?;

    // Two separate counter contracts with different init args should
    // successfully deploy.
    let deploy_nonce1 = 0u64;
    let deploy_nonce2 = 1u64;
    let deployed_contract1_receipt =
        session.call::<_, Result<ContractId, ContractError>>(
            contract_id,
            "simple_deploy",
            &(-64i32, OWNER.to_vec(), deploy_nonce1),
            CONTRACT_DEPLOY_CONTRACT_LIMIT,
        )?;
    let deployed_contract2_receipt =
        session.call::<_, Result<ContractId, ContractError>>(
            contract_id,
            "simple_deploy",
            &(1000i32, OWNER.to_vec(), deploy_nonce2),
            CONTRACT_DEPLOY_CONTRACT_LIMIT,
        )?;

    assert!(deployed_contract1_receipt.gas_spent > EXPECTED_DEPLOY_CHARGE);
    assert!(deployed_contract2_receipt.gas_spent > EXPECTED_DEPLOY_CHARGE);

    let deployed_contract1 = deployed_contract1_receipt.data.unwrap();
    let deployed_contract2 = deployed_contract2_receipt.data.unwrap();

    // Their IDs should be correctly generated.
    assert_eq!(
        deployed_contract1,
        gen_contract_id(CONTRACT_DEPLOYER_TEMPLATE_CODE, deploy_nonce1, &OWNER)
    );
    assert_eq!(
        deployed_contract2,
        gen_contract_id(CONTRACT_DEPLOYER_TEMPLATE_CODE, deploy_nonce2, &OWNER)
    );

    // They should work as expected.
    assert_eq!(
        session
            .call::<_, i32>(deployed_contract1, "read_value", &(), LIMIT)?
            .data,
        -64
    );
    assert_eq!(
        session
            .call::<_, i32>(deployed_contract2, "read_value", &(), LIMIT)?
            .data,
        1000
    );

    session.call::<_, ()>(deployed_contract1, "increment", &(), LIMIT)?;
    session.call::<_, ()>(deployed_contract2, "increment", &(), LIMIT)?;

    assert_eq!(
        session
            .call::<_, i32>(deployed_contract1, "read_value", &(), LIMIT)?
            .data,
        -63
    );
    assert_eq!(
        session
            .call::<_, i32>(deployed_contract2, "read_value", &(), LIMIT)?
            .data,
        1001
    );

    Ok(())
}

#[test]
pub fn contract_deploy_contract_insufficient_gas() -> Result<(), Error> {
    let vm =
        VM::ephemeral_with_session_config(Some(GAS_PER_DEPLOY_BYTE), None)?;

    let bytecode = contract_bytecode!("counter_deployer");
    let contract_id = ContractId::from([1; 32]);
    let mut session = vm.session(SessionData::builder())?;
    session.deploy(
        bytecode,
        ContractData::builder()
            .owner(OWNER)
            .contract_id(contract_id),
        LIMIT,
    )?;

    let deployed_contract1_receipt =
        session.call::<_, Result<ContractId, ContractError>>(
            contract_id,
            "simple_deploy",
            &(-64i32, OWNER.to_vec(), 0u64),
            EXPECTED_DEPLOY_CHARGE - 1,
        )?;

    assert_eq!(
        deployed_contract1_receipt.data,
        Err(ContractError::OutOfGas)
    );

    Ok(())
}

#[test]
pub fn contract_deploy_contract_multiple() -> Result<(), Error> {
    let vm =
        VM::ephemeral_with_session_config(Some(GAS_PER_DEPLOY_BYTE), None)?;

    let bytecode = contract_bytecode!("counter_deployer");
    let contract_id = ContractId::from([1; 32]);

    let mut session = vm.session(SessionData::builder())?;
    session.deploy(
        bytecode,
        ContractData::builder()
            .owner(OWNER)
            .contract_id(contract_id),
        LIMIT,
    )?;

    // Recursively deploying multiple contracts should succeed.
    let deployed_contracts_receipt =
        session.call::<_, Result<Vec<ContractId>, ContractError>>(
            contract_id,
            "multiple_deploy",
            &(-2i32, 2i32, OWNER.to_vec(), 0u64),
            CONTRACT_DEPLOY_CONTRACT_LIMIT * 5,
        )?;

    assert!(deployed_contracts_receipt.gas_spent > EXPECTED_DEPLOY_CHARGE * 5);
    assert!(deployed_contracts_receipt.data.is_ok());

    // Those contracts should work as expected.
    let deployed_contracts = deployed_contracts_receipt.data.unwrap();
    for ((contract, init_value), nonce) in deployed_contracts
        .into_iter()
        .zip([2, 1, 0, -1, -2])
        .zip([4, 3, 2, 1, 0])
    {
        assert_eq!(
            contract,
            gen_contract_id(CONTRACT_DEPLOYER_TEMPLATE_CODE, nonce, &OWNER),
        );

        assert_eq!(
            session
                .call::<_, i32>(contract, "read_value", &(), LIMIT)?
                .data,
            init_value
        );

        session.call::<_, ()>(contract, "increment", &(), LIMIT)?;

        assert_eq!(
            session
                .call::<_, i32>(contract, "read_value", &(), LIMIT)?
                .data,
            init_value + 1
        );
    }

    Ok(())
}

#[test]
pub fn contract_deploy_already_deployed_contract() -> Result<(), Error> {
    let vm =
        VM::ephemeral_with_session_config(Some(GAS_PER_DEPLOY_BYTE), None)?;

    let bytecode = contract_bytecode!("counter_deployer");
    let contract_id = ContractId::from([1; 32]);
    let mut session = vm.session(SessionData::builder())?;
    session.deploy(
        bytecode,
        ContractData::builder()
            .owner(OWNER)
            .contract_id(contract_id),
        LIMIT,
    )?;

    let args = (-64i32, OWNER.to_vec(), 0u64);
    let deployed_contract1_receipt =
        session.call::<_, Result<ContractId, ContractError>>(
            contract_id,
            "simple_deploy",
            &args,
            CONTRACT_DEPLOY_CONTRACT_LIMIT,
        )?;
    let deployed_contract2_receipt =
        session.call::<_, Result<ContractId, ContractError>>(
            contract_id,
            "simple_deploy",
            &args,
            CONTRACT_DEPLOY_CONTRACT_LIMIT,
        )?;

    assert!(deployed_contract1_receipt.gas_spent > EXPECTED_DEPLOY_CHARGE);
    assert!(deployed_contract2_receipt.gas_spent > EXPECTED_DEPLOY_CHARGE);

    assert_eq!(
        deployed_contract2_receipt.data,
        Err(ContractError::InitializationError(
            "Deployed error already exists".to_string()
        ))
    );

    Ok(())
}

#[test]
pub fn contract_deploy_contract_failed_init() -> Result<(), Error> {
    let vm =
        VM::ephemeral_with_session_config(Some(GAS_PER_DEPLOY_BYTE), None)?;

    let bytecode = contract_bytecode!("counter_deployer");

    let contract_id = ContractId::from([1; 32]);
    let mut session = vm.session(SessionData::builder())?;
    session.deploy(
        bytecode,
        ContractData::builder()
            .owner(OWNER)
            .contract_id(contract_id),
        LIMIT,
    )?;

    let deploy_nonce = 1u64;
    let deployed_contract_receipt = session
        .call::<_, Result<ContractId, ContractError>>(
            contract_id,
            "simple_deploy_fail",
            &(-64i32, OWNER.to_vec(), deploy_nonce),
            CONTRACT_DEPLOY_CONTRACT_LIMIT,
        )?;

    assert!(deployed_contract_receipt.gas_spent > EXPECTED_DEPLOY_CHARGE);
    assert!(deployed_contract_receipt.data.is_err());
    assert_eq!(
        deployed_contract_receipt.data,
        Err(ContractError::Panic("Failed to deploy".to_string()))
    );

    let call_result = session.call::<_, i32>(
        gen_contract_id(CONTRACT_DEPLOYER_TEMPLATE_CODE, deploy_nonce, &OWNER),
        "read_value",
        &(),
        LIMIT,
    );
    assert!(matches!(call_result, Err(Error::ContractDoesNotExist(_))), "If a contract's init function fails during deployment, the deployment should be reversed");

    Ok(())
}

#[test]
pub fn contract_deploy_contract_init_deploys() -> Result<(), Error> {
    let vm =
        VM::ephemeral_with_session_config(Some(GAS_PER_DEPLOY_BYTE), None)?;

    let bytecode = contract_bytecode!("counter_deployer");

    let contract_id = ContractId::from([1; 32]);
    let mut session = vm.session(SessionData::builder())?;
    session.deploy(
        bytecode,
        ContractData::builder()
            .owner(OWNER)
            .contract_id(contract_id),
        LIMIT,
    )?;

    // Deploying a contract that deploys other contracts in its init function
    // should succeed.
    let (init_value, fail, fail_at, additional_deploys, deploy_nonce) =
        (-64i32, false, 5u32, 10u32, 1u64);
    let deployed_contract_receipt = session
        .call::<_, Result<ContractId, ContractError>>(
            contract_id,
            "recursive_deploy_through_init",
            &(
                init_value,
                fail,
                fail_at,
                additional_deploys,
                deploy_nonce,
                OWNER.to_vec(),
            ),
            CONTRACT_DEPLOY_CONTRACT_LIMIT * 11,
        )?;

    let expected_successful_deploys = 5;
    let expected_failed_deploys = 1;
    assert!(
        deployed_contract_receipt.gas_spent
            > EXPECTED_DEPLOY_CHARGE
                * (expected_failed_deploys + expected_successful_deploys)
    );

    // The contracts that were successfully deployed should exist.
    let (contract_ids_that_should_exist, contract_id_that_shouldnt_exist) = {
        let mut ids =
            vec![gen_contract_id(CONTRACT_DEPLOYER_TEMPLATE_CODE, 1, &OWNER)];
        let mut shouldnt_exist = vec![];
        let mut fails_from_here = false;
        for deploy_no in (1..=additional_deploys).rev() {
            let id = gen_contract_id(
                CONTRACT_DEPLOYER_TEMPLATE_CODE,
                deploy_no as u64 + 100_000,
                &OWNER,
            );
            if !fails_from_here {
                fails_from_here = deploy_no == fail_at;
            }
            if !fails_from_here {
                ids.push(id);
            } else {
                shouldnt_exist.push(id);
            }
        }
        (ids, shouldnt_exist)
    };

    for contract in contract_ids_that_should_exist {
        assert_eq!(
            session
                .call::<_, i32>(contract, "read_value", &(), LIMIT)?
                .data,
            init_value
        );

        session.call::<_, ()>(contract, "increment", &(), LIMIT)?;

        assert_eq!(
            session
                .call::<_, i32>(contract, "read_value", &(), LIMIT)?
                .data,
            init_value + 1
        );
    }

    // The contract whose init function failed should not exist.
    // Since its init function failed, then all other contracts deployed
    // from it should be reverted.
    for contract in contract_id_that_shouldnt_exist {
        let call_result =
            session.call::<_, i32>(contract, "read_value", &(), LIMIT);
        assert!(matches!(call_result, Err(Error::ContractDoesNotExist(_))));
    }

    Ok(())
}
