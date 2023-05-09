// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{module_bytecode, Error, ModuleData, SessionData, VM};

const OWNER: [u8; 32] = [0u8; 32];

#[test]
pub fn points_get_used() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let counter_id = session
        .deploy(module_bytecode!("counter"), ModuleData::builder(OWNER))?;
    let center_id = session
        .deploy(module_bytecode!("callcenter"), ModuleData::builder(OWNER))?;

    session.call::<_, i64>(counter_id, "read_value", &())?;
    let counter_spent = session.spent();

    session.call::<_, i64>(center_id, "query_counter", &counter_id)?;
    let center_spent = session.spent();

    assert!(counter_spent < center_spent);

    Ok(())
}

#[test]
pub fn fails_with_out_of_points() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let counter_id = session
        .deploy(module_bytecode!("counter"), ModuleData::builder(OWNER))?;

    session.set_point_limit(0);

    let err = session
        .call::<(), i64>(counter_id, "read_value", &())
        .expect_err("should error with no gas");

    assert!(matches!(err, Error::OutOfPoints));

    Ok(())
}

#[test]
pub fn limit_and_spent() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    const LIMIT: u64 = 10000;

    let mut session = vm.session(SessionData::builder())?;

    let spender_id = session
        .deploy(module_bytecode!("spender"), ModuleData::builder(OWNER))?;

    session.set_point_limit(LIMIT);

    let (limit, spent_before, spent_after, called_limit, called_spent) =
        session.call::<_, (u64, u64, u64, u64, u64)>(
            spender_id,
            "get_limit_and_spent",
            &(),
        )?;
    let spender_spent = session.spent();

    assert_eq!(limit, LIMIT, "should be the initial limit");

    println!("=== Spender costs ===");

    println!("limit       : {}", limit);
    println!("spent before: {}", spent_before);
    println!("spent after : {}\n", spent_after);
    println!("called limit: {}", called_limit);
    println!("called spent: {}", called_spent);

    println!("===  Actual cost  ===");
    println!("actual cost : {}", spender_spent);

    Ok(())
}
