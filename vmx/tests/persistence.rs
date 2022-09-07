// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::thread;

use tempfile::tempdir;
use vmx::{module_bytecode, CommitId, Error, VM};

#[test]
fn box_set_get() -> Result<(), Error> {
    let vm = VM::new();

    let mut session = vm.session_mut(None);
    let box_id = session.deploy(module_bytecode!("box"));

    let value = session.query::<(), Option<i16>>(box_id, "get", ())?;
    assert_eq!(value, None);

    session.transact::<i16, ()>(box_id, "set", 0x11)?;

    let value = session.query::<(), Option<i16>>(box_id, "get", ())?;
    assert_eq!(value, Some(0x11));

    Ok(())
}

#[test]
fn box_set_store_restore_get() -> Result<(), Error> {
    let storage_path = tempdir().expect("tmpdir should succeed");

    let (module_id, commit_id) = {
        let mut vm = VM::load(&storage_path)?;

        let mut session = vm.session_mut(None);
        let module_id = session.deploy(module_bytecode!("box"))?;

        session.transact::<i16, ()>()(module_id, "set", 0x23)?;
        let commit_id = vm.commit(session)?;

        (module_id, commit_id)
    };

    let vm = VM::load(storage_path)?;
    let session = vm.session(Some(commit_id))?;

    let value = session.query::<_, Option<i16>>(module_id, "get", ())?;
    assert_eq!(value, Some(0x23));

    Ok(())
}

#[test]
fn box_set_get_concurrent() -> Result<(), Error> {
    let storage_path = tempdir().expect("tmpdir should succeed");

    let mut vm = VM::load(&storage_path)?;

    let (module_id, commit_id) = {
        let mut session = vm.session_mut(None);
        let module_id = session.deploy(module_bytecode!("box"))?;

        session.transact::<i16, ()>()(module_id, "set", 0x23)?;
        let commit_id = vm.commit(session)?;

        (module_id, commit_id)
    };

    const N_THREAD: usize = 8;
    let mut handles = Vec::with_capacity(N_THREAD);
    for _ in 0..N_THREAD {
        handles.push(thread::spawn(|| {
            let session = vm.session(Some(commit_id));
            session.query::<_, Option<i16>>(module_id, "get", ())
        }));
    }

    for handle in handles.drain(..) {
        let value =
            handle.join().expect("joining the thread should succeed")?;
        assert_eq!(value, 0x23);
    }

    Ok(())
}
