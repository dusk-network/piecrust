// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{contract_bytecode, ContractData, Error, SessionData, VM};
use piecrust_uplink::ARGBUF_LEN;

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 40_000_000;

#[test]
fn grow_a_bunch() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(None, SessionData::builder())?;

    let id = session.deploy(
        None,
        contract_bytecode!("grower"),
        &(),
        OWNER,
        LIMIT,
    )?;

    for b in 0..=u8::MAX {
        println!("{b}");
        let bytes = [b; ARGBUF_LEN];
        session.call_raw(id, "append", bytes, LIMIT)?;
    }

    let root = session.commit()?;
    let mut session = vm.session(Some(root), SessionData::builder())?;

    for b in 0..=u8::MAX {
        let offset = b as u32 * ARGBUF_LEN as u32;
        let len = ARGBUF_LEN as u32;

        let offset_bytes = offset.to_le_bytes();
        let len_bytes = len.to_le_bytes();

        let mut call_bytes = [0; 8];

        call_bytes[..4].copy_from_slice(&offset_bytes);
        call_bytes[4..8].copy_from_slice(&len_bytes);

        let receipt = session.call_raw(id, "view", call_bytes, LIMIT)?;

        let bytes = [b; ARGBUF_LEN];
        assert_eq!(receipt.data, bytes);
    }

    Ok(())
}

#[test]
fn error_reverts_growth() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(None, SessionData::builder())?;
    let mut session_err = vm.session(None, SessionData::builder())?;

    let id = session.deploy(
        None,
        contract_bytecode!("grower"),
        &(),
        OWNER,
        LIMIT,
    )?;
    let _ = session_err.deploy(
        None,
        contract_bytecode!("grower"),
        &(),
        OWNER,
        LIMIT,
    )?;

    let initial_len = session
        .memory_len(id)
        .expect("The contract should exist in this session");

    // The first call to `append_error` will result in a call to memory.grow,
    // which must be reverted in both the state, and the size of the memory.
    let bytes = [42; ARGBUF_LEN];
    session
        .call_raw(id, "append", bytes, LIMIT)
        .expect("Appending to the contract should succeed");
    session_err
        .call_raw(id, "append_and_panic", bytes, LIMIT)
        .expect_err("This should error, as per the contract");

    let len_data_err = session_err.call_raw(id, "len", [], LIMIT)?.data;
    assert_eq!(len_data_err.len(), 4);

    let mut len_bytes = [0; 4];
    len_bytes.copy_from_slice(&len_data_err);
    let len = u32::from_le_bytes(len_bytes);

    assert_eq!(len, 0, "There should be nothing appended to the state");

    let final_len = session
        .memory_len(id)
        .expect("The contract should exist in this session");
    let final_len_err = session_err
        .memory_len(id)
        .expect("The contract should exist in this session");

    assert!(initial_len < final_len);
    assert_eq!(
        final_len_err, initial_len,
        "When erroring the length of the memory should revert"
    );

    Ok(())
}
