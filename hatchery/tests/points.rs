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
