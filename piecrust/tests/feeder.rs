// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::sync::mpsc;

use piecrust::{contract_bytecode, ContractData, Error, SessionData, VM};

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;

#[tokio::test(flavor = "multi_thread")]
async fn feed() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let id = session.deploy(
        contract_bytecode!("feeder"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    const FEED_NUM: u32 = 10;
    const GAS_LIMIT: u64 = 1_000_000;

    let (first_sender, receiver) = mpsc::channel();

    session.feeder_call::<_, ()>(
        id,
        "feed_num",
        &FEED_NUM,
        GAS_LIMIT,
        first_sender,
    )?;

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

    let (second_sender, receiver) = mpsc::channel();

    session.feeder_call::<_, ()>(
        id,
        "feed_num_raw",
        &FEED_NUM,
        GAS_LIMIT,
        second_sender,
    )?;

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

#[tokio::test(flavor = "multi_thread")]
async fn feed_errors_when_normal_call() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let id = session.deploy(
        contract_bytecode!("feeder"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    const FEED_NUM: u32 = 10;

    session
        .call::<_, ()>(id, "feed_num", &FEED_NUM, LIMIT)
        .expect_err("Call should error when not called with `feeder_call` or `feeder_call_raw`");

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn feed_out_of_gas() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let id = session.deploy(
        contract_bytecode!("feeder"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    const FEED_NUM: u32 = 100;
    const GAS_LIMIT: u64 = 1_000;

    let (sender, _receiver) = mpsc::channel();

    let err = session
        .feeder_call::<_, ()>(id, "feed_num", &FEED_NUM, GAS_LIMIT, sender)
        .expect_err("Call should error when out of gas");

    assert!(matches!(err, Error::OutOfGas));

    Ok(())
}
