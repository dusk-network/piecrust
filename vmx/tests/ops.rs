// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use dallo::{ModuleId, RawQuery, RawResult, RawTransaction};
use vmx::{module_bytecode, Error, VM};

#[test]
fn debug() -> Result<(), Error> {
    let vm = VM::new();

    let session = vm.session(None);

    let module_id = session.deploy(module_bytecode!("debugger"))?;
    session.query(module_id, "debug", String::from("Hello, World"))?;

    assert_eq!(
        session.debug(),
        &[String::from("What a string! Hello world")]
    );

    Ok(())
}

#[test]
fn height() -> Result<(), Error> {
    let vm = VM::new();
    let mut session = vm.session_mut(None);

    let module_id = session.deploy(module_bytecode!("everest"))?;

    for h in 0..1024 {
        session.set_height(h);
        let height = session.query::<(), u64>(module_id, "get_height", ())?;
        assert_eq!(height, h);
    }

    Ok(())
}

fn hash(buf: &mut [u8], len: u32) -> u32 {
    assert_eq!(len, 4, "the length should come from the module as 4");

    let mut num_bytes = [0; 4];
    num_bytes.copy_from_slice(&buf[..4]);
    let num = i32::from_le_bytes(num_bytes);

    let hash = hash_num(num);
    buf[..32].copy_from_slice(&hash);

    32
}

fn hash_num(num: i32) -> [u8; 32] {
    *blake3::hash(&num.to_le_bytes()).as_bytes()
}

#[test]
fn host_hash() -> Result<(), Error> {
    let vm = {
        let mut vm = VM::new();
        vm.add_host_query("hash", hash);
        vm
    };

    let session = vm.session_mut(None);
    let module_id = session.deploy(module_bytecode!("host"));

    let h = session.query::<i32, [u8; 32]>(module_id, "hash", 42)?;
    assert_eq!(hash_num(42), h);

    Ok(())
}

#[test]
fn events() -> Result<(), Error> {
    let vm = VM::new();

    let mut session = vm.session_mut(None);
    let module_id = session.deploy(module_bytecode!("eventer"));

    const EVENT_NUM: usize = 5;

    session.transact(module_id, "emit_events", EVENT_NUM)?;

    let events = session.events();
    assert_eq!(events.len() as u32, EVENT_NUM);

    for i in 0..EVENT_NUM {
        let index = i as usize;
        assert_eq!(events[index].module_id(), &module_id);
        assert_eq!(events[index].data(), i.lo_le_bytes());
    }

    Ok(())
}

#[test]
fn call_center_read_counter() -> Result<(), Error> {
    let vm = VM::new();

    let mut session = vm.session_mut(None);

    let counter_id = session.deploy(module_bytecode!("counter"));
    let center_id = session.deploy(module_bytecode!("callcenter"))?;

    // read value directly from counter contract, and then through the call
    // center
    let value = session.query::<(), i64>(counter_id, "read_value", ())?;
    let call_center_value = session.query::<ModuleId, i64>(
        center_id,
        "query_counter",
        counter_id,
    )?;

    assert_eq!(0xfc, value);
    assert_eq!(value, call_center_value);

    Ok(())
}

#[test]
fn call_center_counter_direct() -> Result<(), Error> {
    let vm = VM::new();

    let mut session = vm.session_mut(None);

    let counter_id = session.deploy(module_bytecode!("counter"));
    let center_id = session.deploy(module_bytecode!("callcenter"))?;

    // read value directly from counter contract, and then through the call
    // center
    let value = session.query::<(), i64>(counter_id, "read_value", ())?;
    let call_center_value = session.query::<ModuleId, i64>(
        center_id,
        "query_counter",
        counter_id,
    )?;

    assert_eq!(0xfc, value);
    assert_eq!(value, call_center_value);

    session.transact::<ModuleId, ()>(
        center_id,
        "increment_counter",
        counter_id,
    )?;

    let value = session.query::<(), i64>(counter_id, "read_value", ())?;
    let call_center_value = session.query::<ModuleId, i64>(
        center_id,
        "query_counter",
        counter_id,
    )?;

    assert_eq!(0xfd, value);
    assert_eq!(value, call_center_value);

    Ok(())
}

#[test]
fn call_center_counter_delegated() -> Result<(), Error> {
    let vm = VM::new();

    let mut session = vm.session_mut(None);

    let counter_id = session.deploy(module_bytecode!("counter"));
    let center_id = session.deploy(module_bytecode!("callcenter"))?;

    let rq = RawQuery::new("read_value", ());

    let res = session.query::<_, RawQuery>(
        center_id,
        "query_passthrough",
        rq.clone(),
    )?;
    assert_eq!(rq, res);

    // read value through callcenter
    let res = session.query::<_, RawResult>(
        center_id,
        "delegate_query",
        (counter_id, rq),
    )?;
    let value: i64 = res.cast();

    assert_eq!(value, 0xfc);

    let rt = RawTransaction::new("increment", ());

    session.transact::<_, ()>(
        center_id,
        "delegate_transaction",
        (counter_id, rt),
    )?;

    // read value directly
    let value = session.query::<_, i64>(counter_id, "read_value", ())?;
    assert_eq!(value, 0xfd);

    Ok(())
}

#[test]
fn call_center_calls_self() -> Result<(), Error> {
    let vm = VM::new();
    let mut session = vm.session_mut(None);

    let module_id = session.deploy(module_bytecode!("callcenter"))?;

    let calling_self = session.query(module_id, "calling_self", module_id)?;
    assert!(calling_self);

    Ok(())
}

#[test]
fn call_center_caller() -> Result<(), Error> {
    let vm = VM::new();
    let mut session = vm.session_mut(None);

    let module_id = session.deploy(module_bytecode!("callcenter"))?;

    let value = session.query(module_id, "call_self", ())?;
    assert!(value);

    Ok(())
}

#[test]
pub fn limit_and_spent() -> Result<(), Error> {
    let vm = VM::new();
    let mut session = vm.session_mut(None);

    const LIMIT: u64 = 10000;

    session.set_limit(LIMIT);
    let spender_id = session.deploy(module_bytecode!("spender"))?;

    let spender_ret = session.query::<_, (u64, u64, u64, u64, u64)>(
        spender_id,
        "get_limit_and_spent",
        (),
    )?;

    let (limit, spent_before, spent_after, called_limit, called_after) =
        spender_ret;

    assert_eq!(limit, LIMIT, "should be the initial limit");

    println!("=== Spender costs ===");

    println!("limit       : {}", limit);
    println!("spent before: {}", spent_before);
    println!("called limit: {}", called_limit);
    println!("called after: {}", called_after);
    println!("spent after : {}\n", spent_after);

    println!("=== Actual cost  ===");
    println!("actual cost : {}", session.spent());

    Ok(())
}
