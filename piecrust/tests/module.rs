// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use piecrust::{ContractData, Error, SessionData, VM, contract_bytecode};
use piecrust_uplink::ContractId;

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;

fn bytecode_dir(vm: &VM) -> PathBuf {
    vm.root_dir().join("main").join("bytecode")
}

fn module_path(vm: &VM, contract_id: ContractId) -> PathBuf {
    let contract_hex = hex::encode(contract_id);
    bytecode_dir(vm).join(&contract_hex).with_extension("a") // OBJECTCODE_EXTENSION
}

fn module_meta_path(vm: &VM, contract_id: ContractId) -> PathBuf {
    let module = module_path(vm, contract_id);
    module.with_extension("a.meta")
}

fn bytecode_path(vm: &VM, contract_id: ContractId) -> PathBuf {
    let contract_hex = hex::encode(contract_id);
    bytecode_dir(vm).join(&contract_hex)
}

fn dedup_canonical_path(vm: &VM, kind: &str, bytes: &[u8]) -> PathBuf {
    bytecode_dir(vm)
        .join(".dedup")
        .join(kind)
        .join(blake3::hash(bytes).to_hex().to_string())
}

fn dedup_canonical_path_for_file(
    vm: &VM,
    kind: &str,
    path: &Path,
) -> Result<PathBuf, Error> {
    let bytes = std::fs::read(path).map_err(io_to_error)?;
    Ok(dedup_canonical_path(vm, kind, &bytes))
}

fn deploy_counter(vm: &VM) -> Result<(ContractId, [u8; 32]), Error> {
    let mut session = vm.session(SessionData::builder())?;
    let (counter_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    session.call::<_, ()>(counter_id, "increment", &(), LIMIT)?;
    let commit_id = session.commit()?;
    Ok((counter_id, commit_id))
}

fn io_to_error(err: std::io::Error) -> Error {
    Error::PersistenceError(Arc::new(err))
}

#[cfg(unix)]
fn inode(path: &PathBuf) -> Result<u64, Error> {
    Ok(std::fs::metadata(path).map_err(io_to_error)?.ino())
}

#[cfg(unix)]
#[test]
fn duplicate_bytecode_and_modules_share_storage() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let counter_a = ContractId::from_bytes([3; 32]);
    let counter_b = ContractId::from_bytes([4; 32]);
    let mut session = vm.session(SessionData::builder())?;

    session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER).contract_id(counter_a),
        LIMIT,
    )?;
    session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER).contract_id(counter_b),
        LIMIT,
    )?;
    let commit_id = session.commit()?;

    assert_eq!(
        inode(&bytecode_path(&vm, counter_a))?,
        inode(&bytecode_path(&vm, counter_b))?
    );
    assert_eq!(
        inode(&module_path(&vm, counter_a))?,
        inode(&module_path(&vm, counter_b))?
    );
    assert_eq!(
        inode(&module_meta_path(&vm, counter_a))?,
        inode(&module_meta_path(&vm, counter_b))?
    );

    let vm = VM::new(vm.root_dir())?;
    let mut session = vm.session(SessionData::builder().base(commit_id))?;
    assert_eq!(
        session
            .call::<_, i64>(counter_a, "read_value", &(), LIMIT)?
            .data,
        0xfc
    );
    assert_eq!(
        session
            .call::<_, i64>(counter_b, "read_value", &(), LIMIT)?
            .data,
        0xfc
    );

    Ok(())
}

#[test]
fn remove_module() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let (counter_id, commit_id) = deploy_counter(&vm)?;

    let module_file = module_path(&vm, counter_id);
    let module_meta_file = module_meta_path(&vm, counter_id);
    let module_canonical =
        dedup_canonical_path_for_file(&vm, "objectcode", &module_file)?;
    let module_meta_canonical = dedup_canonical_path_for_file(
        &vm,
        "objectcode-meta",
        &module_meta_file,
    )?;
    assert!(module_file.exists());
    assert!(module_meta_file.exists());
    assert!(module_canonical.exists());
    assert!(module_meta_canonical.exists());

    vm.remove_module(counter_id)?;
    assert!(!module_file.exists());
    assert!(!module_meta_file.exists());
    assert!(!module_canonical.exists());
    assert!(!module_meta_canonical.exists());

    vm.recompile_module(counter_id)?;
    assert!(module_file.exists());
    assert!(module_meta_file.exists());
    assert!(
        dedup_canonical_path_for_file(&vm, "objectcode", &module_file)?
            .exists()
    );
    assert!(
        dedup_canonical_path_for_file(
            &vm,
            "objectcode-meta",
            &module_meta_file
        )?
        .exists()
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
fn removing_duplicate_module_keeps_shared_canonical() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let counter_a = ContractId::from_bytes([5; 32]);
    let counter_b = ContractId::from_bytes([6; 32]);
    let mut session = vm.session(SessionData::builder())?;

    session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER).contract_id(counter_a),
        LIMIT,
    )?;
    session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER).contract_id(counter_b),
        LIMIT,
    )?;
    let commit_id = session.commit()?;

    let module_a = module_path(&vm, counter_a);
    let module_b = module_path(&vm, counter_b);
    let meta_a = module_meta_path(&vm, counter_a);
    let meta_b = module_meta_path(&vm, counter_b);
    let module_canonical =
        dedup_canonical_path_for_file(&vm, "objectcode", &module_a)?;
    let meta_canonical =
        dedup_canonical_path_for_file(&vm, "objectcode-meta", &meta_a)?;

    vm.remove_module(counter_a)?;

    assert!(!module_a.exists());
    assert!(!meta_a.exists());
    assert!(module_b.exists());
    assert!(meta_b.exists());
    assert!(module_canonical.exists());
    assert!(meta_canonical.exists());

    vm.recompile_module(counter_a)?;

    let mut session = vm.session(SessionData::builder().base(commit_id))?;
    assert_eq!(
        session
            .call::<_, i64>(counter_a, "read_value", &(), LIMIT)?
            .data,
        0xfc
    );
    assert_eq!(
        session
            .call::<_, i64>(counter_b, "read_value", &(), LIMIT)?
            .data,
        0xfc
    );

    Ok(())
}

#[test]
fn migration_removes_replaced_bytecode_canonical() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (contract, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    session.call::<_, ()>(contract, "increment", &(), LIMIT)?;
    session.call::<_, ()>(contract, "increment", &(), LIMIT)?;
    let root = session.commit()?;

    let old_canonical =
        dedup_canonical_path(&vm, "bytecode", contract_bytecode!("counter"));
    assert!(old_canonical.exists());

    let mut session = vm.session(SessionData::builder().base(root))?;
    session = session.migrate(
        contract,
        contract_bytecode!("double_counter"),
        ContractData::builder(),
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

    let new_canonical = dedup_canonical_path(
        &vm,
        "bytecode",
        contract_bytecode!("double_counter"),
    );
    assert!(!old_canonical.exists());
    assert!(new_canonical.exists());

    let mut session = vm.session(SessionData::builder().base(root))?;
    assert_eq!(
        session
            .call::<_, (i64, i64)>(contract, "read_values", &(), LIMIT)?
            .data,
        (0xfe, 0xcf)
    );

    Ok(())
}

#[test]
fn migration_of_duplicate_bytecode_keeps_shared_canonical() -> Result<(), Error>
{
    let vm = VM::ephemeral()?;
    let counter_a = ContractId::from_bytes([7; 32]);
    let counter_b = ContractId::from_bytes([8; 32]);
    let mut session = vm.session(SessionData::builder())?;

    session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER).contract_id(counter_a),
        LIMIT,
    )?;
    session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER).contract_id(counter_b),
        LIMIT,
    )?;
    let root = session.commit()?;

    let old_canonical =
        dedup_canonical_path(&vm, "bytecode", contract_bytecode!("counter"));
    assert!(old_canonical.exists());

    let mut session = vm.session(SessionData::builder().base(root))?;
    session = session.migrate(
        counter_a,
        contract_bytecode!("double_counter"),
        ContractData::builder(),
        LIMIT,
        |new_contract, session| {
            let old_counter_value = session
                .call::<_, i64>(counter_a, "read_value", &(), LIMIT)?
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

    let new_canonical = dedup_canonical_path(
        &vm,
        "bytecode",
        contract_bytecode!("double_counter"),
    );
    assert!(old_canonical.exists());
    assert!(new_canonical.exists());
    assert!(bytecode_path(&vm, counter_b).exists());

    let mut session = vm.session(SessionData::builder().base(root))?;
    assert_eq!(
        session
            .call::<_, (i64, i64)>(counter_a, "read_values", &(), LIMIT)?
            .data,
        (0xfc, 0xcf)
    );
    assert_eq!(
        session
            .call::<_, i64>(counter_b, "read_value", &(), LIMIT)?
            .data,
        0xfc
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
fn module_cache_recovers_from_missing_or_corrupt_metadata() -> Result<(), Error>
{
    let vm = VM::ephemeral()?;
    let (counter_id, commit_id) = deploy_counter(&vm)?;
    let module_meta_file = module_meta_path(&vm, counter_id);

    std::fs::remove_file(&module_meta_file).map_err(io_to_error)?;

    let vm = VM::new(vm.root_dir())?;
    let mut session = vm.session(SessionData::builder().base(commit_id))?;
    assert_eq!(
        session
            .call::<_, i64>(counter_id, "read_value", &(), LIMIT)?
            .data,
        0xfd
    );
    drop(session);

    assert!(module_meta_file.exists());
    std::fs::write(&module_meta_file, b"broken-metadata")
        .map_err(io_to_error)?;

    let vm = VM::new(vm.root_dir())?;
    let mut session = vm.session(SessionData::builder().base(commit_id))?;
    assert_eq!(
        session
            .call::<_, i64>(counter_id, "read_value", &(), LIMIT)?
            .data,
        0xfd
    );
    drop(session);

    let repaired = std::fs::read(module_meta_file).map_err(io_to_error)?;
    assert_ne!(repaired, b"broken-metadata");

    Ok(())
}

#[test]
fn module_cache_recovers_from_missing_objectcode_with_live_memory_cache()
-> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let (counter_id, commit_id) = deploy_counter(&vm)?;
    let module_file = module_path(&vm, counter_id);
    let module_meta_file = module_meta_path(&vm, counter_id);

    std::fs::remove_file(&module_file).map_err(io_to_error)?;
    assert!(!module_file.exists());
    assert!(module_meta_file.exists());

    let vm = VM::new(vm.root_dir())?;
    assert!(module_file.exists());

    let mut session = vm.session(SessionData::builder().base(commit_id))?;
    assert_eq!(
        session
            .call::<_, i64>(counter_id, "read_value", &(), LIMIT)?
            .data,
        0xfd
    );

    Ok(())
}

#[test]
fn removed_module_stays_unavailable_until_recompiled() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let (counter_id, commit_id) = deploy_counter(&vm)?;

    let module_file = module_path(&vm, counter_id);
    let module_meta_file = module_meta_path(&vm, counter_id);

    vm.remove_module(counter_id)?;
    assert!(!module_file.exists());
    assert!(!module_meta_file.exists());

    let mut session = vm.session(SessionData::builder().base(commit_id))?;
    assert!(
        session
            .call::<_, i64>(counter_id, "read_value", &(), LIMIT)
            .is_err()
    );
    assert!(!module_file.exists());
    assert!(!module_meta_file.exists());

    vm.recompile_module(counter_id)?;

    let mut session = vm.session(SessionData::builder().base(commit_id))?;
    assert_eq!(
        session
            .call::<_, i64>(counter_id, "read_value", &(), LIMIT)?
            .data,
        0xfd
    );

    Ok(())
}

#[test]
fn remove_and_recompile_multiple_contracts() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (counter_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    let (box_id, _) = session.deploy::<_, (), _>(
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
