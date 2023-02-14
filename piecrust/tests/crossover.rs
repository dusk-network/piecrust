// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{module_bytecode, Error, RawTransaction, VM};

#[test]
fn crossover() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.genesis_session();

    let contract_id = session.deploy(module_bytecode!("crossover"))?;

    // Check that initial crossover value is 7, as hardcoded in the module
    assert_eq!(7, session.query::<_, i32>(contract_id, "crossover", &())?);

    // Update crossover, the result is the old value
    let res = session.transact::<_, i32>(contract_id, "set_crossover", &9)?;
    assert_eq!(res, 7);
    assert_eq!(9, session.query::<_, i32>(contract_id, "crossover", &())?);

    // Test self_call_test_a
    let res =
        session.transact::<_, i32>(contract_id, "self_call_test_a", &10)?;
    assert_eq!(res, 9);
    assert_eq!(10, session.query::<_, i32>(contract_id, "crossover", &())?);

    // Test update_and_panic
    let result =
        session.transact::<_, ()>(contract_id, "update_and_panic", &11);
    assert!(result.is_err());
    assert_eq!(10, session.query::<_, i32>(contract_id, "crossover", &())?);

    // Test set_crossover as RawTransaction
    let raw = RawTransaction::new("set_crossover", 12);
    session.transact::<_, ()>(
        contract_id,
        "self_call_test_b",
        &(contract_id, raw),
    )?;
    assert_eq!(12, session.query::<_, i32>(contract_id, "crossover", &())?);

    Ok(())
}
