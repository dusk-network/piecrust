// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

extern crate core;

use std::borrow::Cow;
use std::env;
use std::error::Error;
use std::fmt::Display;
use std::fs;
use std::path::PathBuf;

use piecrust::{CommitId, ModuleId, Session, VM};

static mut PATH: String = String::new();

#[derive(Debug)]
struct IllegalArg;
impl Error for IllegalArg {}

impl Display for IllegalArg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Illegal arg")
    }
}

#[derive(Debug)]
struct PersistE;
impl Error for PersistE {}

impl Display for PersistE {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Persist Error")
    }
}

fn initialize(session: &mut Session) -> Result<(), piecrust::Error> {
    let counter_bytecode = include_bytes!(
        "../../target/wasm32-unknown-unknown/release/counter.wasm"
    );

    let counter_module_id = session.deploy(counter_bytecode)?;

    assert_eq!(
        session.query::<(), i64>(counter_module_id, "read_value", ())?,
        0xfc
    );

    session.transact::<(), ()>(counter_module_id, "increment", ())?;

    assert_eq!(
        session.query::<(), i64>(counter_module_id, "read_value", ())?,
        0xfd
    );

    let module_id_path =
        PathBuf::from(unsafe { &PATH }).join("counter_module_id");
    fs::write(&module_id_path, counter_module_id.as_bytes())
        .map_err(|e| piecrust::Error::PersistenceError(e))?;

    let commit_id_path = PathBuf::from(unsafe { &PATH }).join("commit_id");
    let commit_id = session.commit()?;
    commit_id.persist(commit_id_path)?;

    Ok(())
}

fn confirm(session: &mut Session) -> Result<(), piecrust::Error> {
    let file_path = PathBuf::from(unsafe { &PATH }).join("commit_id");
    let commit_id = CommitId::from(file_path)?;
    session.restore(&commit_id)?;

    let contract_module_id_path =
        PathBuf::from(unsafe { &PATH }).join("counter_module_id");
    let buf = fs::read(&contract_module_id_path)
        .map_err(|e| piecrust::Error::RestoreError(e))?;
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(buf.as_ref());
    let counter_module_id = ModuleId::from(bytes);

    assert_eq!(
        session.query::<(), i64>(counter_module_id, "read_value", ())?,
        0xfd
    );

    Ok(())
}

fn main() -> Result<(), piecrust::Error> {
    let args: Vec<String> = env::args().collect();

    if args.len() != 3 {
        println!("expected: <path> (initialize|confirm|test_both)");
        return Ok(());
    }

    let mut vm: VM = unsafe {
        PATH = args[1].clone();
        VM::new(&PATH).expect("Creating ephemeral VM should work")
    };
    let mut session = vm.session();

    match &*args[2] {
        "initialize" => initialize(&mut session),
        "confirm" => confirm(&mut session),
        "test_both" => {
            initialize(&mut session)?;
            confirm(&mut session)
        }
        _ => Err(piecrust::Error::SessionError(Cow::from("invalid argument"))),
    }
}
