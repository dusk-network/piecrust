// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use vmx::{module_bytecode, Error, VM};

#[test]
pub fn points_get_used() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;

    let counter_id = vm.deploy(module_bytecode!("counter"))?;
    let center_id = vm.deploy(module_bytecode!("callcenter"))?;

    let mut session = vm.session();

    session.query::<_, i64>(counter_id, "read_value", ())?;
    let counter_spent = session.spent();

    session.query::<_, i64>(center_id, "query_counter", counter_id)?;
    let center_spent = session.spent();

    assert!(counter_spent < center_spent);

    Ok(())
}

#[test]
pub fn fails_with_out_of_points() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;

    let counter_id = vm.deploy(module_bytecode!("counter"))?;

    let mut session = vm.session();
    session.set_point_limit(0);

    let err = session
        .query::<(), i64>(counter_id, "read_value", ())
        .expect_err("should error with no gas");

    assert!(matches!(err, Error::OutOfPoints));

    Ok(())
}

#[test]
pub fn limit_and_spent() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;

    const LIMIT: u64 = 10000;

    let spender_id = vm.deploy(module_bytecode!("spender"))?;

    let mut session = vm.session();
    session.set_point_limit(LIMIT);

    let (limit, spent_before, spent_after, called_limit, called_after) =
        session.query::<_, (u64, u64, u64, u64, u64)>(
            spender_id,
            "get_limit_and_spent",
            (),
        )?;
    let spender_spent = session.spent();

    assert_eq!(limit, LIMIT, "should be the initial limit");

    println!("=== Spender costs ===");

    println!("limit       : {}", limit);
    println!("spent before: {}", spent_before);
    println!("called limit: {}", called_limit);
    println!("called after: {}", called_after);
    println!("spent after : {}\n", spent_after);

    println!("=== Actual cost  ===");
    println!("actual cost : {}", spender_spent);

    Ok(())
}
