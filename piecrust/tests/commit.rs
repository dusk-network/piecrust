// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{module_bytecode, DeployData, Error, Session, VM};
use piecrust_uplink::ModuleId;
use std::thread;

const OWNER: [u8; 32] = [0u8; 32];

#[test]
fn read_write_session() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    {
        let mut session = vm.genesis_session();
        let id = session
            .deploy(module_bytecode!("counter"), DeployData::from(OWNER))?;

        assert_eq!(session.query::<(), i64>(id, "read_value", &())?, 0xfc);

        session.transact::<(), ()>(id, "increment", &())?;

        assert_eq!(session.query::<(), i64>(id, "read_value", &())?, 0xfd);
    }

    // mutable session dropped without committing.
    // old counter value still accessible.

    let mut other_session = vm.genesis_session();
    let id = other_session
        .deploy(module_bytecode!("counter"), DeployData::from(OWNER))?;

    assert_eq!(other_session.query::<(), i64>(id, "read_value", &())?, 0xfc);

    other_session.transact::<(), ()>(id, "increment", &())?;

    let _commit_id = other_session.commit()?;

    // session committed, new value accessible

    let mut session = vm.session(_commit_id)?;

    assert_eq!(session.query::<(), i64>(id, "read_value", &())?, 0xfd);
    Ok(())
}

#[test]
fn commit_restore() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session_1 = vm.genesis_session();
    let id = session_1
        .deploy(module_bytecode!("counter"), DeployData::from(OWNER))?;
    // commit 1
    assert_eq!(session_1.query::<(), i64>(id, "read_value", &())?, 0xfc);
    session_1.transact::<(), ()>(id, "increment", &())?;
    let commit_1 = session_1.commit()?;

    // commit 2
    let mut session_2 = vm.session(commit_1)?;
    assert_eq!(session_2.query::<(), i64>(id, "read_value", &())?, 0xfd);
    session_2.transact::<(), ()>(id, "increment", &())?;
    session_2.transact::<(), ()>(id, "increment", &())?;
    let commit_2 = session_2.commit()?;
    let mut session_2 = vm.session(commit_2)?;
    assert_eq!(session_2.query::<(), i64>(id, "read_value", &())?, 0xff);

    // restore commit 1
    let mut session_3 = vm.session(commit_1)?;
    assert_eq!(session_3.query::<(), i64>(id, "read_value", &())?, 0xfd);

    // restore commit 2
    let mut session_4 = vm.session(commit_2)?;
    assert_eq!(session_4.query::<(), i64>(id, "read_value", &())?, 0xff);
    Ok(())
}

#[test]
fn commit_restore_two_modules_session() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.genesis_session();
    let id_1 =
        session.deploy(module_bytecode!("counter"), DeployData::from(OWNER))?;
    let id_2 =
        session.deploy(module_bytecode!("box"), DeployData::from(OWNER))?;

    session.transact::<(), ()>(id_1, "increment", &())?;
    session.transact::<i16, ()>(id_2, "set", &0x11)?;
    assert_eq!(session.query::<(), i64>(id_1, "read_value", &())?, 0xfd);
    assert_eq!(
        session.query::<_, Option<i16>>(id_2, "get", &())?,
        Some(0x11)
    );

    let commit_1 = session.commit()?;

    let mut session = vm.session(commit_1)?;
    session.transact::<(), ()>(id_1, "increment", &())?;
    session.transact::<i16, ()>(id_2, "set", &0x12)?;
    let commit_2 = session.commit()?;
    let mut session = vm.session(commit_2)?;
    assert_eq!(session.query::<(), i64>(id_1, "read_value", &())?, 0xfe);
    assert_eq!(
        session.query::<_, Option<i16>>(id_2, "get", &())?,
        Some(0x12)
    );

    let mut session = vm.session(commit_1)?;

    // check if both modules' state was restored
    assert_eq!(session.query::<(), i64>(id_1, "read_value", &())?, 0xfd);
    assert_eq!(
        session.query::<_, Option<i16>>(id_2, "get", &())?,
        Some(0x11)
    );
    Ok(())
}

#[test]
fn multiple_commits() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.genesis_session();
    let id =
        session.deploy(module_bytecode!("counter"), DeployData::from(OWNER))?;
    // commit 1
    assert_eq!(session.query::<(), i64>(id, "read_value", &())?, 0xfc);
    session.transact::<(), ()>(id, "increment", &())?;
    let commit_1 = session.commit()?;

    // commit 2
    let mut session = vm.session(commit_1)?;
    assert_eq!(session.query::<(), i64>(id, "read_value", &())?, 0xfd);
    session.transact::<(), ()>(id, "increment", &())?;
    session.transact::<(), ()>(id, "increment", &())?;
    let commit_2 = session.commit()?;
    let mut session = vm.session(commit_2)?;
    assert_eq!(session.query::<(), i64>(id, "read_value", &())?, 0xff);

    // restore commit 1
    let mut session = vm.session(commit_1)?;
    assert_eq!(session.query::<(), i64>(id, "read_value", &())?, 0xfd);

    // restore commit 2
    let mut session = vm.session(commit_2)?;
    assert_eq!(session.query::<(), i64>(id, "read_value", &())?, 0xff);
    Ok(())
}

fn increment_counter_and_commit(
    mut session: Session,
    id: ModuleId,
    count: usize,
) -> Result<[u8; 32], Error> {
    for _ in 0..count {
        session.transact::<(), ()>(id, "increment", &())?;
    }
    session.commit()
}

#[test]
fn concurrent_sessions() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.genesis_session();
    let counter =
        session.deploy(module_bytecode!("counter"), DeployData::from(OWNER))?;

    assert_eq!(session.query::<(), i64>(counter, "read_value", &())?, 0xfc);

    let root = session.commit()?;

    let commits = vm.commits();
    assert_eq!(commits.len(), 1, "There should only be one commit");
    assert_eq!(commits[0], root, "The commit should be the received root");

    // spawn different threads incrementing different times and committing
    const THREAD_NUM: usize = 6;
    let mut threads = Vec::with_capacity(THREAD_NUM);
    for n in 0..THREAD_NUM {
        let session = vm.session(root)?;
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
        let session = vm.session(*root)?;
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
fn squashing() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.genesis_session();
    let counter =
        session.deploy(module_bytecode!("counter"), DeployData::from(OWNER))?;

    assert_eq!(session.query::<(), i64>(counter, "read_value", &())?, 0xfc);

    let genesis_root = session.commit()?;

    let session = vm.session(genesis_root)?;
    let root_1 = increment_counter_and_commit(session, counter, 2)?;

    let session = vm.session(root_1)?;
    let root_2 = increment_counter_and_commit(session, counter, 2)?;

    vm.squash_commit(root_1)?;

    let session = vm.session(root_1)?;
    let root_3 = increment_counter_and_commit(session, counter, 2)?;

    assert_eq!(
        root_2, root_3,
        "Squashed commit should produce the same state"
    );

    Ok(())
}
