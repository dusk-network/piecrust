// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;
use std::{fs, thread};

use piecrust::{
    ContractData, Error, Session, SessionData, VM, contract_bytecode,
};
use piecrust_uplink::ContractId;

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;

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

fn has_direct_file(path: &PathBuf) -> bool {
    path.read_dir()
        .map(|mut entries| {
            entries.any(|entry| {
                entry.map(|entry| entry.path().is_file()).unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn read_direct_files(path: &PathBuf) -> BTreeMap<String, Vec<u8>> {
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
    assert_commit_promoted(&paths, &snapshot);

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
