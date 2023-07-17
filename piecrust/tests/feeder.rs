// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::sync::mpsc;

use piecrust::{contract_bytecode, ContractData, Error, SessionData, VM};

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;

#[test]
fn feed() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let id = session.deploy(
        contract_bytecode!("feeder"),
        ContractData::builder(OWNER),
        LIMIT,
    )?;

    const FEED_NUM: u32 = 10;

    let (sender, receiver) = mpsc::channel();

    session.feeder_call::<_, ()>(id, "feed_num", &FEED_NUM, sender)?;

    let numbers = receiver
        .into_iter()
        .map(|data| {
            rkyv::from_bytes(&data).expect("Fed data should be a number")
        })
        .collect::<Vec<u32>>();

    assert_eq!(
        numbers.len(),
        FEED_NUM as usize,
        "The correct number of numbers should be fed"
    );

    for (i, n) in numbers.into_iter().enumerate() {
        assert_eq!(i as u32, n, "Numbers should be fed in order");
    }

    Ok(())
}

#[test]
fn feed_errors_when_normal_call() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let id = session.deploy(
        contract_bytecode!("feeder"),
        ContractData::builder(OWNER),
        LIMIT,
    )?;

    const FEED_NUM: u32 = 10;

    session
        .call::<_, ()>(id, "feed_num", &FEED_NUM, LIMIT)
        .expect_err("Call should error when not called with `feeder_call` or `feeder_call_raw`");

    Ok(())
}
