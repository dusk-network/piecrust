// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

extern crate core;

use std::path::{Path, PathBuf};
use std::{env, fs};

use piecrust::{ModuleId, VM};
const COUNTER_ID: ModuleId = {
    let mut bytes = [0u8; 32];
    bytes[0] = 99;
    ModuleId::from_bytes(bytes)
};

fn initialize_counter<P: AsRef<Path>>(
    vm: &VM,
    commit_id_file_path: P,
) -> Result<(), piecrust::Error> {
    let mut session = vm.genesis_session();

    let counter_bytecode = include_bytes!(
        "../../../../target/wasm32-unknown-unknown/release/counter.wasm"
    );

    session.deploy_with_id(COUNTER_ID, counter_bytecode)?;
    session.transact::<(), ()>(COUNTER_ID, "increment", &())?;

    let commit_root = session.commit()?;
    fs::write(commit_id_file_path, commit_root)
        .expect("writing commit root should succeed");

    Ok(())
}

fn confirm_counter<P: AsRef<Path>>(
    vm: &VM,
    commit_id_file_path: P,
) -> Result<(), piecrust::Error> {
    let mut commit_root = [0u8; 32];

    let commit_root_bytes = fs::read(commit_id_file_path)
        .expect("Reading commit root should succeed");
    commit_root.copy_from_slice(&commit_root_bytes);

    let mut session = vm
        .session(commit_root)
        .expect("Instantiating session from given root should succeed");

    assert_eq!(
        session.query::<(), i64>(COUNTER_ID, "read_value", &())?,
        0xfd
    );

    Ok(())
}

fn initialize<P: AsRef<str>>(vm_data_path: P) -> Result<(), piecrust::Error> {
    let commit_id_file_path =
        PathBuf::from(vm_data_path.as_ref()).join("commit_id");
    let vm = VM::new(vm_data_path.as_ref())?;
    initialize_counter(&vm, &commit_id_file_path)
}

fn confirm<P: AsRef<str>>(vm_data_path: P) -> Result<(), piecrust::Error> {
    let commit_id_file_path =
        PathBuf::from(vm_data_path.as_ref()).join("commit_id");
    let vm = VM::new(vm_data_path.as_ref())?;
    confirm_counter(&vm, &commit_id_file_path)
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
        "confirm" => confirm(&vm_data_path)?,
        "test_both" => {
            initialize(&vm_data_path)?;
            for _ in 0..10 {
                confirm(&vm_data_path)?;
            }
        }
        _ => {
            println!("{}", MESSAGE);
        }
    }

    Ok(())
}
