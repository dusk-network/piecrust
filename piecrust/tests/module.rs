// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{contract_bytecode, ContractData, Error, SessionData, VM};
use piecrust_uplink::ContractId;
use std::path::PathBuf;

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;

fn module_path(vm: &VM, contract_id: ContractId) -> PathBuf {
    let contract_hex = hex::encode(contract_id);
    vm.root_dir()
        .join("main") // MAIN_DIR
        .join("bytecode") // BYTECODE_DIR
        .join(&contract_hex)
        .with_extension("a") // OBJECTCODE_EXTENSION
}

fn deploy_counter(vm: &VM) -> Result<(ContractId, [u8; 32]), Error> {
    let mut session = vm.session(SessionData::builder())?;
    let counter_id = session.deploy(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    session.call::<_, ()>(counter_id, "increment", &(), LIMIT)?;
    let commit_id = session.commit()?;
    Ok((counter_id, commit_id))
}

#[test]
fn remove_module() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let (counter_id, commit_id) = deploy_counter(&vm)?;

    let module_file = module_path(&vm, counter_id);
    assert!(module_file.exists());

    vm.remove_module(counter_id)?;
    assert!(!module_file.exists());

    vm.recompile_module(counter_id)?;
    assert!(module_file.exists());

    let mut session = vm.session(SessionData::builder().base(commit_id))?;
    assert_eq!(
        session
            .call::<_, i64>(counter_id, "read_value", &(), LIMIT)?
            .data,
        0xfd
    );
    session.call::<_, ()>(counter_id, "increment", &(), LIMIT)?;
    assert_eq!(
        session
            .call::<_, i64>(counter_id, "read_value", &(), LIMIT)?
            .data,
        0xfe
    );

    Ok(())
}

#[test]
fn recompile_module() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let (counter_id, commit_id) = deploy_counter(&vm)?;

    let module_file = module_path(&vm, counter_id);
    let original_size = module_file.metadata().unwrap().len();

    std::thread::sleep(std::time::Duration::from_millis(10));
    vm.recompile_module(counter_id)?;

    let new_size = module_file.metadata().unwrap().len();
    assert!(
        new_size > 0
            && new_size > original_size / 2
            && new_size < original_size * 2
    );

    let mut session = vm.session(SessionData::builder().base(commit_id))?;
    assert_eq!(
        session
            .call::<_, i64>(counter_id, "read_value", &(), LIMIT)?
            .data,
        0xfd
    );
    session.call::<_, ()>(counter_id, "increment", &(), LIMIT)?;
    assert_eq!(
        session
            .call::<_, i64>(counter_id, "read_value", &(), LIMIT)?
            .data,
        0xfe
    );

    Ok(())
}

#[test]
fn remove_and_recompile_multiple_contracts() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let counter_id = session.deploy(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    let box_id = session.deploy(
        contract_bytecode!("box"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    session.call::<_, ()>(counter_id, "increment", &(), LIMIT)?;
    session.call::<i16, ()>(box_id, "set", &0x42, LIMIT)?;
    let commit_id = session.commit()?;

    vm.remove_module(counter_id)?;
    vm.remove_module(box_id)?;
    assert!(!module_path(&vm, counter_id).exists());
    assert!(!module_path(&vm, box_id).exists());

    vm.recompile_module(counter_id)?;
    vm.recompile_module(box_id)?;
    assert!(module_path(&vm, counter_id).exists());
    assert!(module_path(&vm, box_id).exists());

    let mut session = vm.session(SessionData::builder().base(commit_id))?;
    assert_eq!(
        session
            .call::<_, i64>(counter_id, "read_value", &(), LIMIT)?
            .data,
        0xfd
    );
    assert_eq!(
        session
            .call::<_, Option<i16>>(box_id, "get", &(), LIMIT)?
            .data,
        Some(0x42)
    );

    Ok(())
}

#[test]
fn recompile_nonexistent_contract() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let fake_id = ContractId::from_bytes([0xff; 32]);

    assert!(vm.recompile_module(fake_id).is_err());
    Ok(())
}

#[test]
fn remove_nonexistent_module() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let fake_id = ContractId::from_bytes([0xff; 32]);

    assert!(vm.remove_module(fake_id).is_ok());
    Ok(())
}
