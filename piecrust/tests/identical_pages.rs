// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::{fs, io, thread};

use piecrust::{ContractData, Error, Session, SessionData, VM};
use piecrust_uplink::ContractId;

const OWNER: [u8; 32] = [0; 32];
const LIMIT: u64 = 1_000_000;
const DEPTH: u32 = 128;
const PAGE_SIZE: usize = 64 * 1024;

const REPLACEMENT_CONTRACT_WAT: &str = r#"
(module
  (memory (export "memory") 4)
  (global (export "A") i32 (i32.const 0))

  (func (export "migrate_state") (param i32) (result i32)
    (i32.store8 (i32.const 65536) (i32.const 7))
    (i32.store8 (i32.const 65536) (i32.const 0))
    (i32.store (i32.const 131072) (i32.const 9))
    (i32.const 0))
  (func (export "ordinary_step") (param i32) (result i32)
    (i32.store8 (i32.const 65536) (i32.const 0))
    (i32.store (i32.const 131072)
      (i32.add (i32.load (i32.const 131072)) (i32.const 1)))
    (i32.const 0))
)
"#;

const PAGE_STATE_CONTRACT_WAT: &str = r#"
(module
  (memory (export "memory") 4)
  (global (export "A") i32 (i32.const 0))

  (func (export "initialize") (param i32) (result i32)
    (i32.store8 (i32.const 65536) (i32.const 1))
    (i32.store (i32.const 131072) (i32.const 1))
    (i32.const 0))
  (func (export "same") (param i32) (result i32)
    (i32.store8 (i32.const 65536) (i32.const 1))
    (i32.const 0))
  (func (export "same_and_change") (param i32) (result i32)
    (i32.store8 (i32.const 65536) (i32.const 1))
    (i32.store (i32.const 131072) (i32.const 2))
    (i32.const 0))
  (func (export "change") (param i32) (result i32)
    (i32.store (i32.const 131072) (i32.const 2))
    (i32.const 0))
  (func (export "change_both") (param i32) (result i32)
    (i32.store8 (i32.const 65536) (i32.const 2))
    (i32.store (i32.const 131072) (i32.const 3))
    (i32.const 0))
  (func (export "transient_to_base") (param i32) (result i32)
    (i32.store8 (i32.const 65536) (i32.const 9))
    (i32.store8 (i32.const 65536) (i32.const 2))
    (i32.store (i32.const 131072) (i32.const 4))
    (i32.const 0))
  (func (export "revert_to_ancestor") (param i32) (result i32)
    (i32.store8 (i32.const 65536) (i32.const 1))
    (i32.store (i32.const 131072) (i32.const 5))
    (i32.const 0))
  (func (export "materialize_zero") (param i32) (result i32)
    (i32.store8 (i32.const 196608) (i32.const 7))
    (i32.store8 (i32.const 196608) (i32.const 0))
    (i32.store (i32.const 131072) (i32.const 6))
    (i32.const 0))
  (func (export "step") (param i32) (result i32)
    (i32.store8 (i32.const 65536) (i32.load8_u (i32.const 65536)))
    (i32.store (i32.const 131072)
      (i32.add (i32.load (i32.const 131072)) (i32.const 1)))
    (i32.const 0))
  (func (export "same_and_arg") (param i32) (result i32)
    (i32.store8 (i32.const 65536) (i32.const 1))
    (i32.store8 (i32.const 131072) (i32.load8_u (i32.const 0)))
    (i32.const 0))
  (func (export "dirty_then_trap") (param i32) (result i32)
    (i32.store8 (i32.const 65536) (i32.const 9))
    (unreachable))
)
"#;

struct CommitPaths {
    memory: PathBuf,
    leaf: PathBuf,
}

fn bytecode() -> Vec<u8> {
    wat::parse_str(PAGE_STATE_CONTRACT_WAT)
        .expect("page-state contract should be valid WAT")
}

fn deploy(vm: &VM) -> Result<(ContractId, [u8; 32]), Error> {
    let mut session = vm.session(SessionData::builder())?;
    let (contract, _) = session.deploy::<_, (), _>(
        &bytecode(),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    session.call_raw(contract, "initialize", [], LIMIT)?;

    Ok((contract, session.commit()?))
}

fn deploy_pair(vm: &VM) -> Result<(ContractId, ContractId, [u8; 32]), Error> {
    let primary = bytecode();
    let alternate = wat::parse_str(PAGE_STATE_CONTRACT_WAT.replacen(
        "(module",
        "(module (func (export \"marker\") (param i32) (result i32) (i32.const 0))",
        1,
    ))
    .expect("alternate page-state contract should be valid WAT");
    let mut session = vm.session(SessionData::builder())?;
    let (first, _) = session.deploy::<_, (), _>(
        &primary,
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    let (second, _) = session.deploy::<_, (), _>(
        &alternate,
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    session.call_raw(first, "initialize", [], LIMIT)?;
    session.call_raw(second, "initialize", [], LIMIT)?;

    Ok((first, second, session.commit()?))
}

fn call_and_commit(
    vm: &VM,
    base: [u8; 32],
    contract: ContractId,
    function: &str,
) -> Result<[u8; 32], Error> {
    let mut session = vm.session(SessionData::builder().base(base))?;
    session.call_raw(contract, function, [], LIMIT)?;
    session.commit()
}

fn page(
    session: &mut Session,
    contract: ContractId,
    index: usize,
) -> Result<Vec<u8>, Error> {
    session.memory_len(contract)?;
    let (bytes, opening) = session
        .memory_pages(contract)
        .expect("contract memory should exist")
        .find_map(|(page_index, bytes, opening)| {
            (page_index == index).then_some((bytes, opening))
        })
        .expect("requested page should be materialized");
    assert!(opening.verify(bytes), "page opening should verify");

    Ok(bytes.to_vec())
}

fn paths(vm: &VM, contract: ContractId, root: [u8; 32]) -> CommitPaths {
    let main = vm.root_dir().join("main");
    let contract = hex::encode(contract.as_bytes());
    let root = hex::encode(root);
    CommitPaths {
        memory: main.join("memory").join(&contract).join(&root),
        leaf: main.join("leaf").join(contract).join(root),
    }
}

fn has_file(path: &Path) -> bool {
    path.read_dir()
        .map(|mut entries| {
            entries.any(|entry| entry.is_ok_and(|entry| entry.path().is_file()))
        })
        .unwrap_or(false)
}

fn assert_empty_hint(paths: &CommitPaths, leaf_exists: bool) {
    assert!(paths.memory.is_dir(), "dirty contract hint should exist");
    assert!(!has_file(&paths.memory), "no page should be persisted");
    assert_eq!(
        paths.leaf.join("element").is_file(),
        leaf_exists,
        "commit leaf presence should match squash state"
    );
}

fn assert_removed(paths: &CommitPaths) {
    assert!(!paths.memory.exists(), "memory delta should be removed");
    assert!(!paths.leaf.exists(), "leaf delta should be removed");
}

#[test]
fn roots_and_page_materialization_match_unfiltered_execution()
-> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let control_vm = VM::ephemeral()?;
    let (contract, initial) = deploy(&vm)?;
    let (control_contract, control_initial) = deploy(&control_vm)?;
    assert_eq!(initial, control_initial);

    let identical = call_and_commit(&vm, initial, contract, "same_and_change")?;
    let control = call_and_commit(
        &control_vm,
        control_initial,
        control_contract,
        "change",
    )?;
    assert_eq!(identical, control, "identical writes must not alter roots");
    assert!(!paths(&vm, contract, identical).memory.join("1").exists());
    assert!(paths(&vm, contract, identical).memory.join("2").is_file());

    let changed = call_and_commit(&vm, identical, contract, "change_both")?;
    assert!(paths(&vm, contract, changed).memory.join("1").is_file());
    let immediate =
        call_and_commit(&vm, changed, contract, "transient_to_base")?;
    assert!(!paths(&vm, contract, immediate).memory.join("1").exists());
    let ancestor =
        call_and_commit(&vm, immediate, contract, "revert_to_ancestor")?;
    assert!(paths(&vm, contract, ancestor).memory.join("1").is_file());

    let zero = call_and_commit(&vm, ancestor, contract, "materialize_zero")?;
    let zero_page = paths(&vm, contract, zero).memory.join("3");
    assert!(
        zero_page.is_file(),
        "an unallocated zero page must be materialized"
    );
    assert_eq!(
        fs::metadata(&zero_page)
            .expect("zero page metadata should exist")
            .len(),
        PAGE_SIZE as u64,
        "an all-hole page must retain its complete logical length"
    );
    assert_eq!(
        fs::read_dir(zero_page.parent().expect("page should have a parent"))
            .expect("page directory should be readable")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.file_name().to_string_lossy().contains("piecrust-tmp")
            })
            .count(),
        0,
        "temporary page files must not survive publication"
    );
    let reopened = VM::new(vm.root_dir())?;
    let mut session = reopened.session(SessionData::builder().base(zero))?;
    assert!(
        page(&mut session, contract, 3)?
            .iter()
            .all(|byte| *byte == 0)
    );

    Ok(())
}

#[test]
fn migrated_fresh_memory_does_not_inherit_finalized_page_indices()
-> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let (contract, helper, initial) = deploy_pair(&vm)?;
    let descendant = call_and_commit(&vm, initial, helper, "step")?;
    vm.finalize_commit(initial)?;

    let finalized_page = vm
        .root_dir()
        .join("main/memory")
        .join(hex::encode(contract.as_bytes()))
        .join("1");
    assert_eq!(
        fs::read(&finalized_page).expect("old finalized page should exist")[0],
        1,
        "the predecessor should have a materialized nonzero page"
    );

    let replacement = wat::parse_str(REPLACEMENT_CONTRACT_WAT)
        .expect("replacement contract should be valid WAT");
    let session = vm.session(SessionData::builder().base(descendant))?;
    let session = session.migrate(
        contract,
        &replacement,
        ContractData::builder(),
        LIMIT,
        |new_contract, session| {
            session.call_raw(new_contract, "migrate_state", [], LIMIT)?;
            Ok(())
        },
    )?;

    let precommit_root = session.root();
    let pages: Vec<_> = session
        .memory_pages(contract)
        .expect("replacement memory should exist")
        .map(|(index, bytes, opening)| (index, bytes.to_vec(), opening))
        .collect();
    assert!(
        !pages.is_empty(),
        "replacement pages should be materialized"
    );
    for (index, bytes, opening) in &pages {
        assert!(
            opening.verify(bytes),
            "replacement page {index} opening should verify precommit bytes"
        );
    }
    let page_one = pages
        .iter()
        .find(|(index, _, _)| *index == 1)
        .expect("replacement page 1 should be materialized");
    assert!(
        page_one.1.iter().all(|byte| *byte == 0),
        "replacement page 1 should contain fresh zero bytes"
    );

    let root = session.commit()?;
    assert_eq!(
        root, precommit_root,
        "precommit and persisted replacement roots should match"
    );
    assert!(
        paths(&vm, contract, root).memory.join("1").is_file(),
        "a new replacement page must not be omitted using old page indices"
    );

    {
        let reopened = VM::new(vm.root_dir())?;
        let mut session =
            reopened.session(SessionData::builder().base(root))?;
        assert!(
            page(&mut session, contract, 1)?
                .iter()
                .all(|byte| *byte == 0),
            "reopened replacement page should not resolve old finalized bytes"
        );
    }

    let ordinary = call_and_commit(&vm, root, contract, "ordinary_step")?;
    assert!(
        !paths(&vm, contract, ordinary).memory.join("1").exists(),
        "an ordinary existing contract should still omit an identical page"
    );

    Ok(())
}

#[test]
fn ancestry_survives_restart_ordered_finalization_and_deletion()
-> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let (contract, initial) = deploy(&vm)?;
    let identical = call_and_commit(&vm, initial, contract, "same_and_change")?;
    let identical_paths = paths(&vm, contract, identical);
    let descendant = call_and_commit(&vm, identical, contract, "step")?;
    let descendant_paths = paths(&vm, contract, descendant);
    assert!(!identical_paths.memory.join("1").exists());
    assert!(!descendant_paths.memory.join("1").exists());

    vm.finalize_commit(initial)?;
    {
        let reopened = VM::new(vm.root_dir())?;
        let mut session =
            reopened.session(SessionData::builder().base(descendant))?;
        assert_eq!(page(&mut session, contract, 1)?[0], 1);
    }
    vm.finalize_commit(identical)?;
    assert_removed(&identical_paths);
    {
        let reopened = VM::new(vm.root_dir())?;
        let mut session =
            reopened.session(SessionData::builder().base(descendant))?;
        assert_eq!(page(&mut session, contract, 1)?[0], 1);
        assert_eq!(
            u32::from_le_bytes(
                page(&mut session, contract, 2)?[..4].try_into().unwrap()
            ),
            3
        );
    }
    vm.delete_commit(descendant)?;
    assert_removed(&descendant_paths);

    Ok(())
}

#[test]
fn all_identical_hints_survive_leaf_retention_and_squashing()
-> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let (identical_contract, changing_contract, initial) = deploy_pair(&vm)?;

    let mut session = vm.session(SessionData::builder().base(initial))?;
    session.call_raw(identical_contract, "same", [], LIMIT)?;
    session.call_raw(changing_contract, "step", [], LIMIT)?;
    let retained = session.commit()?;
    let retained_paths = paths(&vm, identical_contract, retained);
    assert_empty_hint(&retained_paths, true);

    let descendant = call_and_commit(&vm, retained, changing_contract, "step")?;
    vm.finalize_commit(initial)?;
    vm.finalize_commit(retained)?;
    assert_removed(&retained_paths);
    {
        let reopened = VM::new(vm.root_dir())?;
        let mut session =
            reopened.session(SessionData::builder().base(descendant))?;
        assert_eq!(page(&mut session, identical_contract, 1)?[0], 1);
    }

    let expected_root = {
        let mut control =
            vm.session(SessionData::builder().base(descendant))?;
        control.call_raw(changing_contract, "step", [], LIMIT)?;
        control.root()
    };
    let squashed_paths = paths(&vm, identical_contract, expected_root);
    fs::create_dir_all(&squashed_paths.leaf)
        .expect("orphan leaf delta should be created");
    fs::write(squashed_paths.leaf.join("element"), b"stale leaf")
        .expect("orphan leaf element should be written");
    write_unpublished_manifest(&vm, expected_root, &[(identical_contract, 2)]);

    let mut session = vm.session(SessionData::builder().base(descendant))?;
    session.call_raw(identical_contract, "same", [], LIMIT)?;
    session.call_raw(changing_contract, "step", [], LIMIT)?;
    assert_eq!(session.root(), expected_root);
    let squashed = session.commit()?;
    assert_eq!(squashed, expected_root);
    let squashed_paths = paths(&vm, identical_contract, squashed);
    assert_empty_hint(&squashed_paths, false);

    let final_descendant =
        call_and_commit(&vm, squashed, changing_contract, "step")?;
    vm.finalize_commit(descendant)?;
    vm.finalize_commit(squashed)?;
    assert_removed(&squashed_paths);
    let reopened = VM::new(vm.root_dir())?;
    let mut session =
        reopened.session(SessionData::builder().base(final_descendant))?;
    assert_eq!(page(&mut session, identical_contract, 1)?[0], 1);

    Ok(())
}

#[test]
fn legacy_redundant_pages_remain_readable() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let (contract, initial) = deploy(&vm)?;
    let root = call_and_commit(&vm, initial, contract, "same_and_change")?;
    fs::copy(
        paths(&vm, contract, initial).memory.join("1"),
        paths(&vm, contract, root).memory.join("1"),
    )
    .expect("legacy page should be copied");

    let reopened = VM::new(vm.root_dir())?;
    let mut session = reopened.session(SessionData::builder().base(root))?;
    assert_eq!(page(&mut session, contract, 1)?[0], 1);
    session.call_raw(contract, "step", [], LIMIT)?;
    session.commit()?;

    Ok(())
}

fn write_unpublished_manifest(
    vm: &VM,
    root: [u8; 32],
    entries: &[(ContractId, u8)],
) {
    let mut bytes = Vec::with_capacity(48 + entries.len() * 33);
    bytes.extend_from_slice(b"PCRUSTU1");
    bytes.extend_from_slice(&root);
    bytes.extend_from_slice(&(entries.len() as u64).to_le_bytes());
    for (contract, namespaces) in entries {
        bytes.extend_from_slice(contract.as_bytes());
        bytes.push(*namespaces);
    }
    fs::write(
        vm.root_dir()
            .join("main")
            .join(format!(".unpublished-{}", hex::encode(root))),
        bytes,
    )
    .expect("unpublished manifest should be written");
}

#[test]
fn skipped_page_removes_stale_unpublished_delta_from_another_base()
-> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let (contract, initial) = deploy(&vm)?;
    let alternate = call_and_commit(&vm, initial, contract, "change_both")?;

    let target = {
        let mut session = vm.session(SessionData::builder().base(alternate))?;
        session.call_raw(contract, "same_and_change", [], LIMIT)?;
        session.root()
    };
    let target_paths = paths(&vm, contract, target);
    let stale_page = target_paths.memory.join("1");
    let stale_changed_page = target_paths.memory.join("2");
    fs::create_dir_all(&target_paths.memory)
        .expect("orphan memory delta should be created");
    fs::write(&stale_page, vec![0xa5; PAGE_SIZE])
        .expect("a full stale skipped page should be written");
    fs::write(&stale_changed_page, vec![0xa5; PAGE_SIZE])
        .expect("a full stale changed page should be written");
    write_unpublished_manifest(&vm, target, &[(contract, 1)]);
    assert!(
        !vm.root_dir()
            .join("main")
            .join(hex::encode(target))
            .join("base")
            .exists(),
        "the stale delta must not have a publication marker"
    );

    let root = call_and_commit(&vm, initial, contract, "same_and_change")?;
    assert_eq!(root, target, "both bases should reach the same state root");
    assert!(
        !stale_page.exists(),
        "a skipped page must remove stale direct target-root residue"
    );
    let repaired_page = fs::read(&stale_changed_page)
        .expect("the changed page should be rewritten");
    assert_eq!(repaired_page.len(), PAGE_SIZE);
    assert_eq!(&repaired_page[..4], &2u32.to_le_bytes());

    {
        let reopened = VM::new(vm.root_dir())?;
        let mut session =
            reopened.session(SessionData::builder().base(root))?;
        assert_eq!(page(&mut session, contract, 1)?[0], 1);
    }

    let before_duplicate = repaired_page;
    let duplicate = call_and_commit(&vm, initial, contract, "same_and_change")?;
    assert_eq!(duplicate, root);
    assert_eq!(
        fs::read(stale_changed_page)
            .expect("published delta must survive duplicate commit"),
        before_duplicate,
        "the published-root fast path must run before cleanup"
    );

    Ok(())
}

#[test]
fn retry_removes_stale_root_data_for_contract_absent_from_attempt()
-> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let (first, second, initial) = deploy_pair(&vm)?;
    let alternate = call_and_commit(&vm, initial, first, "change_both")?;

    let target = {
        let mut session = vm.session(SessionData::builder().base(initial))?;
        session.call_raw(first, "change_both", [], LIMIT)?;
        session.call_raw(second, "same_and_change", [], LIMIT)?;
        session.root()
    };
    let stale = paths(&vm, first, target);
    let commit_dir = vm.root_dir().join("main").join(hex::encode(target));
    fs::create_dir_all(&stale.memory).expect("stale memory directory");
    fs::create_dir_all(&stale.leaf).expect("stale leaf directory");
    fs::create_dir_all(&commit_dir).expect("stale commit directory");
    fs::write(stale.memory.join("1"), vec![0xa5; PAGE_SIZE])
        .expect("stale memory page");
    fs::write(stale.leaf.join("element"), b"stale element")
        .expect("stale leaf element");
    fs::write(commit_dir.join("stale"), b"stale commit metadata")
        .expect("stale commit metadata");
    write_unpublished_manifest(&vm, target, &[(first, 3)]);

    let root = call_and_commit(&vm, alternate, second, "same_and_change")?;
    assert_eq!(root, target, "both attempts should reach the same root");
    assert!(
        !stale.memory.exists(),
        "retry must remove memory residue for an absent contract"
    );
    assert!(
        !stale.leaf.exists(),
        "retry must remove leaf residue for an absent contract"
    );
    assert!(
        !commit_dir.join("stale").exists() && commit_dir.join("base").is_file(),
        "retry must replace the complete unpublished commit namespace"
    );

    let reopened = VM::new(vm.root_dir())?;
    let mut session = reopened.session(SessionData::builder().base(root))?;
    assert_eq!(
        page(&mut session, first, 1)?[0],
        2,
        "the absent contract must resolve through the retry base"
    );

    Ok(())
}

#[test]
fn cleanup_failure_prevents_publication() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let (contract, initial) = deploy(&vm)?;
    let target = {
        let mut session = vm.session(SessionData::builder().base(initial))?;
        session.call_raw(contract, "same_and_change", [], LIMIT)?;
        session.root()
    };
    let target_paths = paths(&vm, contract, target);
    fs::create_dir_all(
        target_paths
            .memory
            .parent()
            .expect("target delta should have a parent"),
    )
    .expect("contract memory directory should exist");
    fs::write(&target_paths.memory, b"not a directory")
        .expect("an obstructing regular file should be written");

    call_and_commit(&vm, initial, contract, "same_and_change")
        .expect_err("delta cleanup errors must prevent publication");
    assert!(
        !vm.root_dir()
            .join("main")
            .join(hex::encode(target))
            .join("base")
            .exists(),
        "cleanup failure must occur before the publication marker"
    );

    fs::remove_file(&target_paths.memory)
        .expect("the obstruction should be removable");
    assert_eq!(
        call_and_commit(&vm, initial, contract, "same_and_change")?,
        target,
        "the commit should remain retryable after cleanup succeeds"
    );

    Ok(())
}

#[test]
fn publication_failure_leaves_no_visible_commit_and_remains_retryable()
-> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let control_vm = VM::ephemeral()?;
    let (contract, initial) = deploy(&vm)?;
    let (control_contract, control_initial) = deploy(&control_vm)?;
    let expected = call_and_commit(
        &control_vm,
        control_initial,
        control_contract,
        "same_and_change",
    )?;

    let main = vm.root_dir().join("main");
    let original_mode = fs::metadata(&main)
        .expect("main directory should exist")
        .permissions()
        .mode();
    fs::set_permissions(&main, fs::Permissions::from_mode(0o555))
        .expect("main directory should become read-only");
    let failed = call_and_commit(&vm, initial, contract, "same_and_change");
    fs::set_permissions(
        &main,
        fs::Permissions::from_mode(original_mode & 0o7777),
    )
    .expect("main directory permissions should be restored");
    assert!(
        matches!(failed, Err(Error::PersistenceError(io_error)) if io_error.kind() == io::ErrorKind::PermissionDenied)
    );

    let partial_paths = paths(&vm, contract, expected);
    assert!(!main.join(hex::encode(expected)).exists());
    assert!(!partial_paths.memory.join("1").exists());
    assert!(!partial_paths.memory.join("2").exists());
    let reopened = VM::new(vm.root_dir())?;
    assert_eq!(reopened.commits(), vec![initial]);
    drop(reopened);

    let retried = call_and_commit(&vm, initial, contract, "same_and_change")?;
    assert_eq!(retried, expected);
    let reopened = VM::new(vm.root_dir())?;
    let mut session = reopened.session(SessionData::builder().base(retried))?;
    assert_eq!(page(&mut session, contract, 1)?[0], 1);
    assert_eq!(
        u32::from_le_bytes(
            page(&mut session, contract, 2)?[..4].try_into().unwrap()
        ),
        2
    );

    Ok(())
}

#[test]
fn identical_pages_resolve_through_deep_ancestry() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let (contract, mut root) = deploy(&vm)?;
    for _ in 0..DEPTH {
        root = call_and_commit(&vm, root, contract, "step")?;
        assert!(!paths(&vm, contract, root).memory.join("1").exists());
    }

    let reopened = VM::new(vm.root_dir())?;
    let mut session = reopened.session(SessionData::builder().base(root))?;
    assert_eq!(page(&mut session, contract, 1)?[0], 1);
    assert_eq!(
        u32::from_le_bytes(
            page(&mut session, contract, 2)?[..4].try_into().unwrap()
        ),
        DEPTH + 1
    );

    Ok(())
}

#[test]
fn concurrent_commit_decisions_are_isolated() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let (contract, initial) = deploy(&vm)?;
    let mut workers = Vec::new();
    for value in 2u8..=5 {
        let mut session = vm.session(SessionData::builder().base(initial))?;
        workers.push(thread::spawn(move || -> Result<_, Error> {
            session.call_raw(contract, "same_and_arg", vec![value], LIMIT)?;
            Ok((value, session.commit()?))
        }));
    }

    let results = workers
        .into_iter()
        .map(|worker| worker.join().expect("worker should not panic"))
        .collect::<Result<Vec<_>, _>>()?;
    for (value, root) in results {
        assert!(!paths(&vm, contract, root).memory.join("1").exists());
        let reopened = VM::new(vm.root_dir())?;
        let mut session =
            reopened.session(SessionData::builder().base(root))?;
        assert_eq!(page(&mut session, contract, 1)?[0], 1);
        assert_eq!(page(&mut session, contract, 2)?[0], value);
    }

    Ok(())
}

#[test]
fn rolled_back_writes_do_not_become_commit_dirty_pages() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let control_vm = VM::ephemeral()?;
    let (contract, initial) = deploy(&vm)?;
    let (control_contract, control_initial) = deploy(&control_vm)?;

    let mut session = vm.session(SessionData::builder().base(initial))?;
    session
        .call_raw(contract, "dirty_then_trap", [], LIMIT)
        .expect_err("the trapping call should fail");
    session.call_raw(contract, "change", [], LIMIT)?;
    let root = session.commit()?;
    let control = call_and_commit(
        &control_vm,
        control_initial,
        control_contract,
        "change",
    )?;
    assert_eq!(root, control);
    assert!(!paths(&vm, contract, root).memory.join("1").exists());

    Ok(())
}
