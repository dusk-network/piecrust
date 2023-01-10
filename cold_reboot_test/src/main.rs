// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

extern crate core;

use std::env;
use std::path::PathBuf;

use piecrust::{CommitId, Session, VM};

static mut PATH: String = String::new();

fn initialize(session: &mut Session) -> Result<(), piecrust::Error> {
    let counter_bytecode = include_bytes!(
        "../../target/wasm32-unknown-unknown/release/counter.wasm"
    );

    let module_id = session.deploy(counter_bytecode)?;

    assert_eq!(session.query::<(), i64>(module_id, "read_value", ())?, 0xfc);

    session.transact::<(), ()>(module_id, "increment", ())?;

    assert_eq!(session.query::<(), i64>(module_id, "read_value", ())?, 0xfd);

    let commit_id_path = PathBuf::from(unsafe { &PATH }).join("commit_id");
    let commit_id = session.commit()?;
    commit_id.persist(commit_id_path)?;

    Ok(())
}

fn confirm(session: &mut Session) -> Result<(), piecrust::Error> {
    let file_path = PathBuf::from(unsafe { &PATH }).join("commit_id");
    let commit_id = CommitId::from(file_path)?;
    session.restore(&commit_id)?;

    let counter_bytecode = include_bytes!(
        "../../target/wasm32-unknown-unknown/release/counter.wasm"
    );
    /*
     * Note that module deployment does not change its state.
     */
    let module_id = session.deploy(counter_bytecode)?;

    assert_eq!(session.query::<(), i64>(module_id, "read_value", ())?, 0xfd);

    Ok(())
}

fn main() -> Result<(), piecrust::Error> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!("argument expected: <path_for_vm_data>");
        return Ok(());
    }

    let mut vm: VM = unsafe {
        PATH = args[1].clone();
        VM::new(&PATH)?
    };
    let mut session = vm.session();

    initialize(&mut session)?;

    vm.persist()?;

    /*
     * Simulate cold reboot - construct new VM in the same location.
     */
    let mut vm_rebooted: VM = unsafe { VM::new(&PATH)? };
    let mut session_rebooted = vm_rebooted.session();

    confirm(&mut session_rebooted)
}
