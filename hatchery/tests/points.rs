// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use hatchery::{module_bytecode, Error, Receipt, World};

#[test]
pub fn points_get_used() -> Result<(), Error> {
    let mut world = World::ephemeral()?;

    let counter_id = world.deploy(module_bytecode!("counter"))?;
    let center_id = world.deploy(module_bytecode!("callcenter"))?;

    let receipt_counter: Receipt<i64> =
        world.query(counter_id, "read_value", ())?;
    let receipt_center: Receipt<i64> =
        world.query(center_id, "query_counter", counter_id)?;

    assert!(receipt_counter.points_used() < receipt_center.points_used());

    Ok(())
}

#[test]
pub fn fails_with_out_of_points() -> Result<(), Error> {
    let mut world = World::ephemeral()?;

    world.set_point_limit(0);
    let counter_id = world.deploy(module_bytecode!("counter"))?;

    let err = world
        .query::<(), i64>(counter_id, "read_value", ())
        .expect_err("should error with no gas");

    assert!(matches!(err, Error::OutOfPoints(mid) if mid == counter_id));

    Ok(())
}

#[test]
pub fn limit_and_spent() -> Result<(), Error> {
    let mut world = World::ephemeral()?;

    const LIMIT: u64 = 10000;

    world.set_point_limit(LIMIT);
    let spender_id = world.deploy(module_bytecode!("spender"))?;

    let receipt_spender: Receipt<(u64, u64, u64, u64, u64)> =
        world.query(spender_id, "get_limit_and_spent", ())?;

    let (limit, spent_before, spent_after, called_limit, called_after) =
        *receipt_spender;

    assert_eq!(limit, LIMIT, "should be the initial limit");

    println!("=== Spender costs ===");

    println!("limit       : {}", limit);
    println!("spent before: {}", spent_before);
    println!("called limit: {}", called_limit);
    println!("called after: {}", called_after);
    println!("spent after : {}\n", spent_after);

    println!("=== Actual cost  ===");
    println!("actual cost : {}", receipt_spender.points_used());

    Ok(())
}
