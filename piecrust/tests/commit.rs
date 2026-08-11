// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;
use std::{fs, io, thread};

use piecrust::{
    ContractData, Error, Session, SessionData, VM, contract_bytecode,
};
use piecrust_uplink::ContractId;

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;

const ZERO_WRITE_CONTRACT: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x06, 0x01, 0x60,
    0x01, 0x7f, 0x01, 0x7f, 0x03, 0x02, 0x01, 0x00, 0x05, 0x03, 0x01, 0x00,
    0x01, 0x06, 0x06, 0x01, 0x7f, 0x00, 0x41, 0x00, 0x0b, 0x07, 0x14, 0x03,
    0x06, 0x6d, 0x65, 0x6d, 0x6f, 0x72, 0x79, 0x02, 0x00, 0x01, 0x41, 0x03,
    0x00, 0x03, 0x72, 0x75, 0x6e, 0x00, 0x00, 0x0a, 0x06, 0x01, 0x04, 0x00,
    0x20, 0x00, 0x0b,
];

#[test]
fn read_write_session() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    {
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
    }

    // mutable session dropped without committing.
    // old counter value still accessible.

    let mut other_session = vm.session(SessionData::builder())?;
    let (id, _) = other_session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    assert_eq!(
        other_session
            .call::<_, i64>(id, "read_value", &(), LIMIT)?
            .data,
        0xfc
    );

    other_session.call::<_, ()>(id, "increment", &(), LIMIT)?;

    let _commit_id = other_session.commit()?;

    // session committed, new value accessible

    let mut session = vm.session(SessionData::builder().base(_commit_id))?;

    assert_eq!(
        session.call::<_, i64>(id, "read_value", &(), LIMIT)?.data,
        0xfd
    );
    Ok(())
}

#[test]
fn commit_restore() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session_1 = vm.session(SessionData::builder())?;
    let (id, _) = session_1.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    // commit 1
    assert_eq!(
        session_1.call::<_, i64>(id, "read_value", &(), LIMIT)?.data,
        0xfc
    );
    session_1.call::<_, ()>(id, "increment", &(), LIMIT)?;
    let commit_1 = session_1.commit()?;

    // commit 2
    let mut session_2 = vm.session(SessionData::builder().base(commit_1))?;
    assert_eq!(
        session_2.call::<_, i64>(id, "read_value", &(), LIMIT)?.data,
        0xfd
    );
    session_2.call::<_, ()>(id, "increment", &(), LIMIT)?;
    session_2.call::<_, ()>(id, "increment", &(), LIMIT)?;
    let commit_2 = session_2.commit()?;
    let mut session_2 = vm.session(SessionData::builder().base(commit_2))?;
    assert_eq!(
        session_2.call::<_, i64>(id, "read_value", &(), LIMIT)?.data,
        0xff
    );

    // restore commit 1
    let mut session_3 = vm.session(SessionData::builder().base(commit_1))?;
    assert_eq!(
        session_3.call::<_, i64>(id, "read_value", &(), LIMIT)?.data,
        0xfd
    );

    // restore commit 2
    let mut session_4 = vm.session(SessionData::builder().base(commit_2))?;
    assert_eq!(
        session_4.call::<_, i64>(id, "read_value", &(), LIMIT)?.data,
        0xff
    );
    Ok(())
}

#[test]
fn commit_restore_two_contracts_session() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;
    let (id_1, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    let (id_2, _) = session.deploy::<_, (), _>(
        contract_bytecode!("box"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    session.call::<_, ()>(id_1, "increment", &(), LIMIT)?;
    session.call::<i16, ()>(id_2, "set", &0x11, LIMIT)?;
    assert_eq!(
        session.call::<_, i64>(id_1, "read_value", &(), LIMIT)?.data,
        0xfd
    );
    assert_eq!(
        session
            .call::<_, Option<i16>>(id_2, "get", &(), LIMIT)?
            .data,
        Some(0x11)
    );

    let commit_1 = session.commit()?;

    let mut session = vm.session(SessionData::builder().base(commit_1))?;
    session.call::<_, ()>(id_1, "increment", &(), LIMIT)?;
    session.call::<i16, ()>(id_2, "set", &0x12, LIMIT)?;
    let commit_2 = session.commit()?;
    let mut session = vm.session(SessionData::builder().base(commit_2))?;
    assert_eq!(
        session.call::<_, i64>(id_1, "read_value", &(), LIMIT)?.data,
        0xfe
    );
    assert_eq!(
        session
            .call::<_, Option<i16>>(id_2, "get", &(), LIMIT)?
            .data,
        Some(0x12)
    );

    let mut session = vm.session(SessionData::builder().base(commit_1))?;

    // check if both contracts' state was restored
    assert_eq!(
        session
            .call::<(), i64>(id_1, "read_value", &(), LIMIT)?
            .data,
        0xfd
    );
    assert_eq!(
        session
            .call::<_, Option<i16>>(id_2, "get", &(), LIMIT)?
            .data,
        Some(0x11)
    );
    Ok(())
}

#[test]
fn multiple_commits() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;
    let (id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    // commit 1
    assert_eq!(
        session.call::<(), i64>(id, "read_value", &(), LIMIT)?.data,
        0xfc
    );
    session.call::<(), ()>(id, "increment", &(), LIMIT)?;
    let commit_1 = session.commit()?;

    // commit 2
    let mut session = vm.session(SessionData::builder().base(commit_1))?;
    assert_eq!(
        session.call::<(), i64>(id, "read_value", &(), LIMIT)?.data,
        0xfd
    );
    session.call::<(), ()>(id, "increment", &(), LIMIT)?;
    session.call::<(), ()>(id, "increment", &(), LIMIT)?;
    let commit_2 = session.commit()?;
    let mut session = vm.session(SessionData::builder().base(commit_2))?;
    assert_eq!(
        session.call::<(), i64>(id, "read_value", &(), LIMIT)?.data,
        0xff
    );

    // restore commit 1
    let mut session = vm.session(SessionData::builder().base(commit_1))?;
    assert_eq!(
        session.call::<(), i64>(id, "read_value", &(), LIMIT)?.data,
        0xfd
    );

    // restore commit 2
    let mut session = vm.session(SessionData::builder().base(commit_2))?;
    assert_eq!(
        session.call::<(), i64>(id, "read_value", &(), LIMIT)?.data,
        0xff
    );
    Ok(())
}

#[test]
fn root_equal_on_err() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let (callcenter_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    let (counter_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let root = session.commit()?;

    let mut session_after = vm.session(SessionData::builder().base(root))?;
    let mut session_after_alt =
        vm.session(SessionData::builder().base(root))?;

    assert_eq!(
        session_after.root(),
        session_after_alt.root(),
        "Roots should be equal at the beginning"
    );

    session_after
        .call::<_, ()>(callcenter_id, "panik", &counter_id, LIMIT)
        .expect_err("Calling with too little gas should error");

    assert_eq!(
        session_after.root(),
        session_after_alt.root(),
        "Roots should be equal immediately after erroring call"
    );

    session_after.call::<_, ()>(
        callcenter_id,
        "increment_counter",
        &counter_id,
        LIMIT,
    )?;
    session_after_alt.call::<_, ()>(
        callcenter_id,
        "increment_counter",
        &counter_id,
        LIMIT,
    )?;

    assert_eq!(
        session_after.root(),
        session_after_alt.root(),
        "Roots should be equal after call"
    );

    Ok(())
}

fn increment_counter_and_commit(
    mut session: Session,
    id: ContractId,
    count: usize,
) -> Result<[u8; 32], Error> {
    for _ in 0..count {
        session.call::<(), ()>(id, "increment", &(), LIMIT)?;
    }
    session.commit()
}

struct CommitPaths {
    memory: PathBuf,
    commit_memory: PathBuf,
    leaf_element: PathBuf,
    commit_leaf: PathBuf,
}

struct CommitSnapshot {
    memory_pages: BTreeMap<String, Vec<u8>>,
    leaf_element: Vec<u8>,
}

fn commit_paths(vm: &VM, contract: ContractId, root: [u8; 32]) -> CommitPaths {
    let contract = hex::encode(contract.as_bytes());
    let root = hex::encode(root);
    let main = vm.root_dir().join("main");
    let memory = main.join("memory").join(&contract);
    let leaf = main.join("leaf").join(&contract);

    CommitPaths {
        commit_memory: memory.join(&root),
        memory,
        leaf_element: leaf.join("element"),
        commit_leaf: leaf.join(&root),
    }
}

fn committed_counter() -> Result<(VM, ContractId, [u8; 32]), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;
    let (contract, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    let root = session.commit()?;

    Ok((vm, contract, root))
}

fn committed_zero_write_contract() -> Result<(VM, ContractId, [u8; 32]), Error>
{
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;
    let (contract, _) = session.deploy::<_, (), _>(
        ZERO_WRITE_CONTRACT,
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    let root = session.commit()?;

    Ok((vm, contract, root))
}

fn child_commit(vm: &VM, root: [u8; 32]) -> Result<[u8; 32], Error> {
    let mut session = vm.session(SessionData::builder().base(root))?;
    session.deploy::<_, (), _>(
        contract_bytecode!("everest"),
        ContractData::builder().owner([1; 32]),
        LIMIT,
    )?;
    session.commit()
}

fn hide_commit_metadata(vm: &VM, root: [u8; 32]) -> (PathBuf, PathBuf) {
    let commit_dir = vm.root_dir().join("main").join(hex::encode(root));
    let base_info = commit_dir.join("base");
    let base_info_backup = commit_dir.join("base.test-backup");
    fs::rename(&base_info, &base_info_backup)
        .expect("commit metadata should be hidden for fault injection");

    (base_info, base_info_backup)
}

fn restore_commit_metadata(base_info: &Path, backup: &Path) {
    fs::rename(backup, base_info)
        .expect("commit metadata should be restored for retry");
}

fn has_direct_file(path: &Path) -> bool {
    path.read_dir()
        .map(|mut entries| {
            entries.any(|entry| {
                entry.map(|entry| entry.path().is_file()).unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn read_direct_files(path: &Path) -> BTreeMap<String, Vec<u8>> {
    path.read_dir()
        .expect("directory should be readable")
        .map(|entry| {
            let entry = entry.expect("directory entry should be readable");
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let contents = fs::read(path).expect("file should be readable");

            (name, contents)
        })
        .collect()
}

fn snapshot_commit(paths: &CommitPaths) -> CommitSnapshot {
    let memory_pages = read_direct_files(&paths.commit_memory);
    assert!(
        !memory_pages.is_empty(),
        "commit-scoped memory pages should exist"
    );

    CommitSnapshot {
        memory_pages,
        leaf_element: fs::read(paths.commit_leaf.join("element"))
            .expect("commit-scoped leaf element should be readable"),
    }
}

fn assert_commit_paths_exist(paths: &CommitPaths) {
    assert!(
        has_direct_file(&paths.commit_memory),
        "commit-scoped memory pages should exist"
    );
    assert!(
        paths.commit_leaf.join("element").is_file(),
        "commit-scoped leaf element should exist"
    );
}

fn assert_commit_paths_removed(paths: &CommitPaths) {
    assert!(
        !paths.commit_memory.exists(),
        "commit-scoped memory path should be removed"
    );
    assert!(
        !paths.commit_leaf.exists(),
        "commit-scoped leaf path should be removed"
    );
}

fn assert_commit_promoted(paths: &CommitPaths, snapshot: &CommitSnapshot) {
    assert_commit_paths_removed(paths);
    for (page, contents) in &snapshot.memory_pages {
        let promoted_page = paths.memory.join(page);
        assert_eq!(
            fs::read(&promoted_page)
                .expect("finalized memory page should exist"),
            *contents,
            "finalized memory page should match commit-scoped page"
        );
    }
    assert_eq!(
        fs::read(&paths.leaf_element)
            .expect("finalized leaf element should exist"),
        snapshot.leaf_element,
        "finalized leaf element should match commit-scoped leaf element"
    );
}

fn assert_commit_deleted(paths: &CommitPaths) {
    assert_commit_paths_removed(paths);
    assert!(
        !has_direct_file(&paths.memory),
        "deleted memory pages should not be promoted"
    );
    assert!(
        !paths.leaf_element.exists(),
        "deleted leaf element should not be promoted"
    );
}

fn assert_waiting_for_session_drop<T>(rx: &mpsc::Receiver<T>) {
    assert!(
        rx.recv_timeout(Duration::from_millis(500)).is_err(),
        "commit operation should wait while the base session is held"
    );
}

fn assert_started(rx: &mpsc::Receiver<()>) {
    rx.recv_timeout(Duration::from_secs(1))
        .expect("commit operation worker should start");
}

#[test]
fn concurrent_sessions() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;
    let (counter, _) = session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    assert_eq!(
        session
            .call::<(), i64>(counter, "read_value", &(), LIMIT)?
            .data,
        0xfc
    );

    let root = session.commit()?;

    let commits = vm.commits();
    assert_eq!(commits.len(), 1, "There should only be one commit");
    assert_eq!(commits[0], root, "The commit should be the received root");

    // spawn different threads incrementing different times and committing
    const THREAD_NUM: usize = 6;
    let mut threads = Vec::with_capacity(THREAD_NUM);
    for n in 0..THREAD_NUM {
        let session = vm.session(SessionData::builder().base(root))?;
        threads.push(thread::spawn(move || {
            increment_counter_and_commit(session, counter, n + 1)
        }));
    }

    let mut roots: Vec<[u8; 32]> = threads
        .into_iter()
        .map(|handle| {
            handle.join().unwrap().expect("Committing should succeed")
        })
        .collect();

    let num_commits = roots.len();

    roots.sort();
    roots.dedup();

    assert_eq!(num_commits, roots.len(), "Commits should all be different");

    let commits = vm.commits();
    assert_eq!(
        commits.len(),
        THREAD_NUM + 1,
        "There should be the genesis commit plus the ones just made"
    );

    // start sessions with all the commits and do lots of increments just to
    // waste time
    const INCREMENTS_NUM: usize = 100;
    let mut threads = Vec::with_capacity(roots.len());
    for root in &roots {
        let session = vm.session(SessionData::builder().base(*root))?;
        threads.push(thread::spawn(move || {
            increment_counter_and_commit(session, counter, INCREMENTS_NUM)
        }));
    }

    // Try and delete all the commits while they're working
    for root in roots {
        vm.delete_commit(root)?;
    }

    let mut roots: Vec<[u8; 32]> = threads
        .into_iter()
        .map(|handle| {
            handle.join().unwrap().expect("Committing should succeed")
        })
        .collect();

    let num_commits = roots.len();

    roots.sort();
    roots.dedup();

    assert_eq!(num_commits, roots.len(), "Commits should all be different");

    let commits = vm.commits();
    assert_eq!(
        commits.len(),
        THREAD_NUM + 1,
        "The deleted commits should not be returned"
    );

    Ok(())
}

#[test]
fn finalize_commit_promotes_commit_state() -> Result<(), Error> {
    let (vm, contract, root) = committed_counter()?;
    let paths = commit_paths(&vm, contract, root);
    assert_commit_paths_exist(&paths);
    let snapshot = snapshot_commit(&paths);

    vm.finalize_commit(root)?;

    assert!(
        !vm.commits().contains(&root),
        "finalized root should not remain an unfinalized commit"
    );
    assert!(
        !vm.root_dir().join("main/.completed-operations").exists(),
        "successful finalization should not retain recovery metadata"
    );
    assert_commit_promoted(&paths, &snapshot);

    Ok(())
}

#[test]
fn finalize_commit_with_squashed_contract_hint() -> Result<(), Error> {
    const DIRTY_SAME_CONTRACT: &str = r#"
        (module
          (memory (export "memory") 1)
          (global (export "A") i32 (i32.const 0))
          (func (export "same") (param i32) (result i32)
            (i32.store8 (i32.const 4096) (i32.const 1))
            (i32.store8 (i32.const 4096) (i32.const 0))
            (i32.const 0)))
    "#;
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;
    let bytecode = wat::parse_str(DIRTY_SAME_CONTRACT)
        .expect("dirty-same contract should compile");
    let (contract, _) = session.deploy::<_, (), _>(
        &bytecode,
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    session.call_raw(contract, "same", [], LIMIT)?;
    let initial = session.commit()?;

    let mut session = vm.session(SessionData::builder().base(initial))?;
    session.deploy::<_, (), _>(
        contract_bytecode!("everest"),
        ContractData::builder().owner([1; 32]),
        LIMIT,
    )?;
    let descendant = session.commit()?;
    vm.finalize_commit(initial)?;

    let mut session = vm.session(SessionData::builder().base(descendant))?;
    session.call_raw(contract, "same", [], LIMIT)?;
    session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner([2; 32]),
        LIMIT,
    )?;
    let root = session.commit()?;

    let paths = commit_paths(&vm, contract, root);
    assert!(
        !paths.commit_leaf.join("element").exists(),
        "an unchanged contract index should be squashed"
    );

    vm.finalize_commit(descendant)?;
    vm.finalize_commit(root)?;

    Ok(())
}

#[test]
fn finalize_commit_accepts_new_contract_without_dirty_memory_pages()
-> Result<(), Error> {
    let (vm, contract, root) = committed_zero_write_contract()?;
    let paths = commit_paths(&vm, contract, root);
    assert!(
        paths.commit_memory.is_dir(),
        "zero-write deployment should have an empty commit-scoped memory directory"
    );
    assert!(
        !has_direct_file(&paths.commit_memory),
        "zero-write deployment should not have commit-scoped memory pages"
    );
    assert!(
        paths.commit_leaf.join("element").is_file(),
        "deployment should have a commit-scoped index element"
    );

    let mut child_session = vm.session(SessionData::builder().base(root))?;
    child_session.deploy::<_, (), _>(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    let child_root = child_session.commit()?;

    vm.finalize_commit(root)?;

    assert!(
        !vm.commits().contains(&root),
        "finalized root should not remain an unfinalized commit"
    );
    assert!(
        paths.leaf_element.is_file(),
        "finalized contract index element should be promoted"
    );

    let reopened = VM::new(vm.root_dir())?;
    let mut session =
        reopened.session(SessionData::builder().base(child_root))?;
    assert_eq!(
        session.call::<u32, u32>(contract, "run", &7, LIMIT)?.data,
        7,
        "finalized zero-write contract should remain callable after reopen"
    );

    Ok(())
}

#[test]
fn finalize_commit_accepts_legacy_zero_write_layout() -> Result<(), Error> {
    let (vm, contract, root) = committed_zero_write_contract()?;
    let child_root = child_commit(&vm, root)?;
    let paths = commit_paths(&vm, contract, root);
    fs::remove_dir(&paths.commit_memory)
        .expect("legacy zero-write layout should omit the empty directory");

    vm.finalize_commit(root)?;

    let reopened = VM::new(vm.root_dir())?;
    let mut session =
        reopened.session(SessionData::builder().base(child_root))?;
    assert_eq!(
        session.call::<u32, u32>(contract, "run", &7, LIMIT)?.data,
        7,
        "legacy zero-write contracts should remain callable"
    );

    Ok(())
}

#[cfg(unix)]
#[test]
fn finalize_rejects_broken_memory_directory_symlink() -> Result<(), Error> {
    use std::os::unix::fs::symlink;

    let (vm, contract, root) = committed_zero_write_contract()?;
    let paths = commit_paths(&vm, contract, root);
    fs::remove_dir(&paths.commit_memory)
        .expect("empty memory directory should be removable");
    symlink("missing.test-target", &paths.commit_memory)
        .expect("broken memory-directory symlink should be created");

    assert!(
        vm.finalize_commit(root).is_err(),
        "a broken memory-directory symlink must not look like a legacy zero-write layout"
    );
    assert!(vm.commits().contains(&root));

    fs::remove_file(&paths.commit_memory)
        .expect("broken memory-directory symlink should be removed");
    fs::create_dir(&paths.commit_memory)
        .expect("valid empty memory directory should be restored");
    vm.finalize_commit(root)?;

    Ok(())
}

#[test]
fn failed_finalize_retains_commit_for_retry() -> Result<(), Error> {
    let (vm, contract, root) = committed_counter()?;
    let paths = commit_paths(&vm, contract, root);
    let snapshot = snapshot_commit(&paths);
    let (base_info, base_info_backup) = hide_commit_metadata(&vm, root);

    assert!(
        vm.finalize_commit(root).is_err(),
        "finalization should fail while commit metadata is unavailable"
    );
    assert!(
        vm.commits().contains(&root),
        "a preflight failure should retain the commit for retry"
    );
    assert_commit_paths_exist(&paths);

    restore_commit_metadata(&base_info, &base_info_backup);
    vm.finalize_commit(root)?;

    assert!(
        !vm.commits().contains(&root),
        "successful retry should remove the unfinalized commit"
    );
    assert_commit_promoted(&paths, &snapshot);

    Ok(())
}

#[test]
fn delete_commit_accepts_new_contract_without_dirty_memory_pages()
-> Result<(), Error> {
    let (vm, contract, root) = committed_zero_write_contract()?;
    let paths = commit_paths(&vm, contract, root);
    assert!(
        paths.commit_memory.is_dir(),
        "zero-write deployment should have an empty commit-scoped memory directory"
    );
    assert!(
        !has_direct_file(&paths.commit_memory),
        "zero-write deployment should not have commit-scoped memory pages"
    );

    vm.delete_commit(root)?;

    assert!(
        !vm.commits().contains(&root),
        "deleted root should not remain in the commit store"
    );
    assert_commit_paths_removed(&paths);

    Ok(())
}

#[test]
fn failed_delete_retains_commit_for_retry() -> Result<(), Error> {
    let (vm, contract, root) = committed_counter()?;
    let paths = commit_paths(&vm, contract, root);
    let (base_info, base_info_backup) = hide_commit_metadata(&vm, root);

    assert!(
        vm.delete_commit(root).is_err(),
        "deletion should fail while commit metadata is unavailable"
    );
    assert!(
        !vm.commits().contains(&root),
        "a pending deletion should hide the commit until retry"
    );
    assert_commit_paths_exist(&paths);

    restore_commit_metadata(&base_info, &base_info_backup);
    vm.delete_commit(root)?;

    assert!(
        !vm.commits().contains(&root),
        "successful retry should remove the commit"
    );
    assert_commit_deleted(&paths);

    Ok(())
}

#[test]
fn late_finalize_failure_is_recoverable_and_blocks_sessions()
-> Result<(), Error> {
    let (vm, contract, root) = committed_counter()?;
    let child_root = child_commit(&vm, root)?;
    let paths = commit_paths(&vm, contract, root);
    let blocker = paths.commit_memory.join("unexpected.test-directory");
    fs::create_dir(&blocker).expect("late-failure blocker should be created");

    assert!(
        vm.finalize_commit(root).is_err(),
        "finalization should fail on unexpected commit-scoped state"
    );
    assert!(
        !vm.commits().contains(&root),
        "a pending operation should hide its root"
    );
    assert!(
        vm.session(SessionData::builder()).is_err(),
        "sessions should be blocked while recovery is pending"
    );
    assert!(
        vm.session(SessionData::builder().base(root)).is_err(),
        "the partially finalized root should not remain executable"
    );

    fs::remove_dir(blocker).expect("late-failure blocker should be removed");
    vm.finalize_commit(root)?;

    let reopened = VM::new(vm.root_dir())?;
    let mut session =
        reopened.session(SessionData::builder().base(child_root))?;
    assert_eq!(
        session
            .call::<(), i64>(contract, "read_value", &(), LIMIT)?
            .data,
        0xfc,
        "retry should publish the committed state"
    );

    Ok(())
}

#[test]
fn late_delete_failure_is_recoverable_and_blocks_sessions() -> Result<(), Error>
{
    let (vm, _contract, root) = committed_counter()?;
    let paths = commit_paths(&vm, _contract, root);
    let leaf_backup = paths.commit_leaf.with_extension("test-backup");
    fs::rename(&paths.commit_leaf, &leaf_backup)
        .expect("leaf directory should be moved for fault injection");
    fs::write(&paths.commit_leaf, b"not a directory")
        .expect("leaf blocker should be created");

    assert!(vm.delete_commit(root).is_err());
    assert!(!vm.commits().contains(&root));
    assert!(vm.session(SessionData::builder()).is_err());
    assert!(
        vm.session(SessionData::builder().base(root)).is_err(),
        "the partially deleted root should not remain executable"
    );

    fs::remove_file(&paths.commit_leaf)
        .expect("leaf blocker should be removed");
    fs::rename(leaf_backup, &paths.commit_leaf)
        .expect("leaf directory should be restored");
    vm.delete_commit(root)?;

    assert!(!vm.commits().contains(&root));
    assert_commit_deleted(&paths);

    Ok(())
}

#[test]
fn finalize_rejects_missing_dirty_pages_and_allows_retry() -> Result<(), Error>
{
    let (vm, contract, root) = committed_counter()?;
    let child_root = child_commit(&vm, root)?;
    let paths = commit_paths(&vm, contract, root);
    let memory_backup = paths.commit_memory.with_extension("test-backup");
    fs::rename(&paths.commit_memory, &memory_backup)
        .expect("dirty pages should be moved for fault injection");

    assert!(
        vm.finalize_commit(root).is_err(),
        "missing dirty pages should be rejected"
    );
    assert!(vm.commits().contains(&root));

    fs::rename(memory_backup, &paths.commit_memory)
        .expect("dirty pages should be restored");
    vm.finalize_commit(root)?;

    let mut session = vm.session(SessionData::builder().base(child_root))?;
    assert_eq!(
        session
            .call::<(), i64>(contract, "read_value", &(), LIMIT)?
            .data,
        0xfc,
        "retry should publish the original committed bytes"
    );

    Ok(())
}

#[test]
fn deferred_operations_stop_after_the_first_failure() -> Result<(), Error> {
    let (vm, contract, root) = committed_counter()?;
    let child_root = child_commit(&vm, root)?;
    let paths = commit_paths(&vm, contract, root);
    let blocker = paths.commit_memory.join("unexpected.test-directory");
    fs::create_dir(&blocker).expect("late-failure blocker should be created");

    let held_session = vm.session(SessionData::builder().base(root))?;
    let (finalize_started_tx, finalize_started_rx) = mpsc::channel();
    let (finalize_tx, finalize_rx) = mpsc::channel();
    let (delete_started_tx, delete_started_rx) = mpsc::channel();
    let (delete_tx, delete_rx) = mpsc::channel();

    thread::scope(|scope| {
        scope.spawn(|| {
            finalize_started_tx.send(()).unwrap();
            finalize_tx.send(vm.finalize_commit(root)).unwrap();
        });
        assert_started(&finalize_started_rx);
        thread::sleep(Duration::from_millis(50));

        scope.spawn(|| {
            delete_started_tx.send(()).unwrap();
            delete_tx.send(vm.delete_commit(root)).unwrap();
        });
        assert_started(&delete_started_rx);
        thread::sleep(Duration::from_millis(50));

        drop(held_session);

        assert!(
            finalize_rx
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .is_err(),
            "the first deferred operation should report its failure"
        );
        let delete_error = delete_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .expect_err("the next deferred operation should be skipped");
        assert!(
            matches!(
                delete_error,
                Error::PersistenceError(error)
                    if error.kind() == io::ErrorKind::Interrupted
            ),
            "the skipped operation should report an interrupted persistence error"
        );
    });

    fs::remove_dir(blocker).expect("late-failure blocker should be removed");
    vm.finalize_commit(root)?;
    let mut session = vm.session(SessionData::builder().base(child_root))?;
    assert_eq!(
        session
            .call::<(), i64>(contract, "read_value", &(), LIMIT)?
            .data,
        0xfc,
        "the skipped delete must not consume the finalized state"
    );

    Ok(())
}

#[test]
fn startup_resumes_pending_finalization() -> Result<(), Error> {
    let (vm, contract, root) = committed_counter()?;
    let child_root = child_commit(&vm, root)?;
    let root_dir = vm.root_dir().to_path_buf();
    let paths = commit_paths(&vm, contract, root);
    let blocker = paths.commit_memory.join("unexpected.test-directory");
    fs::create_dir(&blocker).expect("late-failure blocker should be created");

    assert!(vm.finalize_commit(root).is_err());
    let marker = root_dir
        .join("main")
        .join(hex::encode(root))
        .join(".finalize");
    assert!(
        marker.is_file(),
        "a failed finalization should leave a recovery marker"
    );
    assert_eq!(
        marker
            .metadata()
            .expect("marker metadata should exist")
            .len(),
        0,
        "the operation should be fully encoded by the marker name"
    );
    fs::remove_dir(blocker).expect("late-failure blocker should be removed");
    drop(vm);

    let reopened = VM::new(&root_dir)?;
    assert!(
        !reopened.commits().contains(&root),
        "startup should complete rather than reload the pending finalization"
    );
    let mut session =
        reopened.session(SessionData::builder().base(child_root))?;
    assert_eq!(
        session
            .call::<(), i64>(contract, "read_value", &(), LIMIT)?
            .data,
        0xfc,
        "startup should resume and complete finalization"
    );

    Ok(())
}

#[test]
fn startup_resumes_pending_deletion() -> Result<(), Error> {
    let (vm, contract, root) = committed_counter()?;
    let root_dir = vm.root_dir().to_path_buf();
    let paths = commit_paths(&vm, contract, root);
    let leaf_backup = paths.commit_leaf.with_extension("test-backup");
    fs::rename(&paths.commit_leaf, &leaf_backup)
        .expect("leaf directory should be moved for fault injection");
    fs::write(&paths.commit_leaf, b"not a directory")
        .expect("leaf blocker should be created");

    assert!(vm.delete_commit(root).is_err());
    let marker = root_dir
        .join("main")
        .join(hex::encode(root))
        .join(".delete");
    assert!(marker.is_file(), "a failed deletion should leave a marker");
    assert_eq!(
        marker
            .metadata()
            .expect("marker metadata should exist")
            .len(),
        0,
        "the operation should be fully encoded by the marker name"
    );
    fs::remove_file(&paths.commit_leaf)
        .expect("leaf blocker should be removed");
    fs::rename(leaf_backup, &paths.commit_leaf)
        .expect("leaf directory should be restored");
    drop(vm);

    let reopened = VM::new(&root_dir)?;
    assert!(
        !reopened.commits().contains(&root),
        "startup should resume and complete deletion"
    );
    assert_commit_deleted(&paths);

    Ok(())
}

#[test]
fn delete_commit_waits_for_held_session_then_removes_state() -> Result<(), Error>
{
    let (vm, contract, root) = committed_counter()?;
    let paths = commit_paths(&vm, contract, root);
    assert_commit_paths_exist(&paths);

    let held_session = vm.session(SessionData::builder().base(root))?;
    let (started_tx, started_rx) = mpsc::channel();
    let (tx, rx) = mpsc::channel();

    thread::scope(|scope| {
        scope.spawn(|| {
            started_tx
                .send(())
                .expect("started receiver should still be alive");
            tx.send(vm.delete_commit(root))
                .expect("result receiver should still be alive");
        });

        assert_started(&started_rx);
        assert_waiting_for_session_drop(&rx);
        drop(held_session);

        rx.recv_timeout(Duration::from_secs(2))
            .expect("delete should finish after the base session is dropped")
    })?;

    assert!(
        !vm.commits().contains(&root),
        "deleted root should not remain an unfinalized commit"
    );
    assert_commit_deleted(&paths);

    Ok(())
}

#[test]
fn finalize_commit_waits_for_held_session_then_promotes_state()
-> Result<(), Error> {
    let (vm, contract, root) = committed_counter()?;
    let paths = commit_paths(&vm, contract, root);
    assert_commit_paths_exist(&paths);
    let snapshot = snapshot_commit(&paths);

    let held_session = vm.session(SessionData::builder().base(root))?;
    let (started_tx, started_rx) = mpsc::channel();
    let (tx, rx) = mpsc::channel();

    thread::scope(|scope| {
        scope.spawn(|| {
            started_tx
                .send(())
                .expect("started receiver should still be alive");
            tx.send(vm.finalize_commit(root))
                .expect("result receiver should still be alive");
        });

        assert_started(&started_rx);
        assert_waiting_for_session_drop(&rx);
        drop(held_session);

        rx.recv_timeout(Duration::from_secs(2))
            .expect("finalize should finish after the base session is dropped")
    })?;

    assert!(
        !vm.commits().contains(&root),
        "finalized root should not remain an unfinalized commit"
    );
    assert_commit_promoted(&paths, &snapshot);

    Ok(())
}

fn make_session(vm: &VM) -> Result<(Session, ContractId), Error> {
    const HEIGHT: u64 = 29_000u64;
    let mut session =
        vm.session(SessionData::builder().insert("height", HEIGHT)?)?;
    let (contract_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("everest"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    Ok((session, contract_id))
}

#[test]
fn session_move() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let (mut session, contract_id) = make_session(&vm)?;

    // This tests that a session can be moved without subsequent calls producing
    // a SIGSEGV. The pattern is very common downstream, and should be tested
    // for.
    session.call::<_, u64>(contract_id, "get_height", &(), LIMIT)?;

    Ok(())
}
