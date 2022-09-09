// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use vmx::{module_bytecode, Error, VM};

#[test]
pub fn points_get_used() -> Result<(), Error> {
    let vm = VM::new();
    let mut session = vm.session_mut(None)?;

    let counter_id = session.deploy(module_bytecode!("counter"))?;

    session.query::<_, i64>(counter_id, "read_value", ())?;
    let spent = session.spent();
    session.query::<_, i64>(counter_id, "read_value", ())?;

    assert!(spent < session.spent());

    Ok(())
}

#[test]
pub fn fails_with_out_of_points() -> Result<(), Error> {
    let vm = VM::new();
    let mut session = vm.session_mut(None)?;

    session.set_limit(0);
    let counter_id = session.deploy(module_bytecode!("counter"))?;

    let err = session
        .query::<(), i64>(counter_id, "read_value", ())
        .expect_err("query should error");

    assert!(matches!(err, Error::OutOfPoints(mid) if mid == counter_id));

    Ok(())
}
