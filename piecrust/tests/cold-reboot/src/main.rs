// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

extern crate core;

use std::env;
use std::path::{Path, PathBuf};

use piecrust::{CommitId, Session, VM};

fn initialize_counter<P: AsRef<Path>>(
    vm: &mut VM,
    commit_id_file_path: P,
) -> Result<(), piecrust::Error> {
    let mut session = vm.session();

    let counter_bytecode = include_bytes!(
        "../../../../target/wasm32-unknown-unknown/release/counter.wasm"
    );

    let module_id = session.deploy(counter_bytecode)?;

    // assert_eq!(
    //     session.query::<(), i64>(module_id, "read_value", &())?,
    //     0xfc
    // );
    session.transact::<(), ()>(module_id, "increment", &())?;
    // assert_eq!(
    //     session.query::<(), i64>(module_id, "read_value", &())?,
    //     0xfd
    // );

    let commit_id = session.commit()?;
    assert_eq!(commit_id.as_bytes(), vm.session().root(false)?);
    commit_id.persist(commit_id_file_path.as_ref())?;

    vm.persist()?;

    let mut session2 = vm.session();
    let module_id = session2.deploy(counter_bytecode)?;
    session2.transact::<(), ()>(module_id, "increment", &())?;
    let commit_id2 = session2.commit()?;
    assert_eq!(commit_id2.as_bytes(), vm.session().root(false)?);
    commit_id2.persist(commit_id_file_path.as_ref())?;
    vm.persist()?;

    let mut session3 = vm.session();
    let module_id = session3.deploy(counter_bytecode)?;
    session3.transact::<(), ()>(module_id, "increment", &())?;
    let commit_id3 = session3.commit()?;
    assert_eq!(commit_id3.as_bytes(), vm.session().root(false)?);
    commit_id3.persist(commit_id_file_path.as_ref())?;
    vm.persist()?;

    let mut session4 = vm.session();
    let module_id = session4.deploy(counter_bytecode)?;
    session4.transact::<(), ()>(module_id, "increment", &())?;
    let commit_id4 = session4.commit()?;
    assert_eq!(commit_id4.as_bytes(), vm.session().root(false)?);
    commit_id4.persist(commit_id_file_path.as_ref())?;
    vm.persist()?;

    Ok(())
}

fn confirm_counter<P: AsRef<Path>>(
    session: &mut Session,
    commit_id_file_path: P,
    expected: i64
) -> Result<(), piecrust::Error> {
    let commit_id = CommitId::restore(commit_id_file_path)?;
    session.restore(&commit_id)?;
    assert_eq!(commit_id.as_bytes(), session.root(false)?);

    let counter_bytecode = include_bytes!(
        "../../../../target/wasm32-unknown-unknown/release/counter.wasm"
    );

    /*
     * Note that module deployment does not change its state.
     */
    let module_id = session.deploy(counter_bytecode)?;

    assert_eq!(
        session.query::<(), i64>(module_id, "read_value", &())?,
        expected
    );

    Ok(())
}

fn initialize<P: AsRef<str>>(vm_data_path: P) -> Result<(), piecrust::Error> {
    let commit_id_file_path =
        PathBuf::from(vm_data_path.as_ref()).join("commit_id");
    let mut vm = VM::new(vm_data_path.as_ref())?;
    initialize_counter(&mut vm, &commit_id_file_path)
}

fn confirm<P: AsRef<str>>(vm_data_path: P, expected: i64) -> Result<(), piecrust::Error> {
    let commit_id_file_path =
        PathBuf::from(vm_data_path.as_ref()).join("commit_id");
    let mut vm = VM::new(vm_data_path.as_ref())?;
    let mut session = vm.session();
    confirm_counter(&mut session, &commit_id_file_path, expected)
}

fn main() -> Result<(), piecrust::Error> {
    const MESSAGE: &str =
        "argument expected: <path_for_vm_data> (initialize|confirm|test_both)";
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        println!("{}", MESSAGE);
        return Ok(());
    }

    let vm_data_path = args[1].clone();

    match &*args[2] {
        "initialize" => initialize(&vm_data_path)?,
        "confirm" => confirm(&vm_data_path, 0xfd)?,
        "test_both" => {
            let mut expected = 0xfd;
            for _ in 0..10 {
                initialize(&vm_data_path)?;
                confirm(&vm_data_path, expected)?;
                expected += 1;
            }
        }
        _ => {
            println!("{}", MESSAGE);
        }
    }

    Ok(())
}
