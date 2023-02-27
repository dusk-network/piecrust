// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{module_bytecode, Error, VM};
use piecrust_uplink::ModuleId;

const CROSSOVER_ONE: ModuleId = {
    let mut bytes = [0; 32];
    bytes[0] = 0x01;
    ModuleId::from_bytes(bytes)
};

const CROSSOVER_TWO: ModuleId = {
    let mut bytes = [0; 32];
    bytes[0] = 0x02;
    ModuleId::from_bytes(bytes)
};

#[test]
fn crossover() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.genesis_session();

    session.deploy_with_id(CROSSOVER_ONE, module_bytecode!("crossover"))?;
    session.deploy_with_id(CROSSOVER_TWO, module_bytecode!("crossover"))?;

    const CROSSOVER_TO_SET: i32 = 10;

    let state_is_ok: bool = session.transact(
        CROSSOVER_ONE,
        "call_panicking_and_set",
        &(CROSSOVER_TWO, CROSSOVER_TO_SET),
    )?;

    assert!(
        state_is_ok,
        "The state should be unchanged in the panicking call's ray"
    );
    assert_eq!(
        session.query::<_, i32>(CROSSOVER_ONE, "crossover", &())?,
        CROSSOVER_TO_SET,
        "The state should be properly set even after a call panics"
    );

    Ok(())
}
