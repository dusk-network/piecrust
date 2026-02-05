// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{ContractData, Error, SessionData, VM, contract_bytecode};

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;

#[test]
fn out_of_bounds() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let c_example_id = session.deploy(
        contract_bytecode!("c_example"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    session
        .call::<_, ()>(c_example_id, "out_of_bounds", &(), LIMIT)
        .expect_err("An out of bounds access should error");

    Ok(())
}

#[test]
fn not_out_of_bounds() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let c_example_id = session.deploy(
        contract_bytecode!("c_example"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    // 0xFFFF_FFFF + 2 would overflow in wasm32, but in wasm64 it's
    // 0x1_0000_0001 which is well within the 4TB memory map.
    session
        .call::<_, ()>(c_example_id, "not_out_of_bounds", &(), LIMIT)
        .expect("A wasm64 access within 4TB should succeed");

    Ok(())
}

#[test]
fn bad_contract() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let _ = session
        .deploy(
            contract_bytecode!("invalid"),
            ContractData::builder().owner(OWNER),
            LIMIT,
        )
        .expect_err("Deploying an invalid contract should error");

    Ok(())
}
