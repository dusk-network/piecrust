// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{
    ContractData, Error, Session, SessionData, VM, contract_bytecode,
};
use piecrust_uplink::{ARGBUF_LEN, ContractId};

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;
const VALID_RETURN_LEN: i32 = 4;

fn deploy_badreturn() -> Result<(Session, ContractId), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;
    let (id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("badreturn"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    Ok((session, id))
}

fn read_counter(session: &mut Session, id: ContractId) -> Result<u32, Error> {
    Ok(session.call::<_, u32>(id, "counter", &(), LIMIT)?.data)
}

#[test]
fn top_level_valid_return_len_is_accepted() -> Result<(), Error> {
    let (mut session, id) = deploy_badreturn()?;
    assert_eq!(read_counter(&mut session, id)?, 72);

    let receipt = session.call_raw(
        id,
        "raw_return_len",
        VALID_RETURN_LEN.to_le_bytes(),
        LIMIT,
    )?;

    assert_eq!(receipt.data.len(), VALID_RETURN_LEN as usize);
    assert_eq!(read_counter(&mut session, id)?, 73);

    Ok(())
}

#[test]
fn top_level_negative_return_len_is_rejected() -> Result<(), Error> {
    let (mut session, id) = deploy_badreturn()?;
    assert_eq!(read_counter(&mut session, id)?, 72);

    let err = session
        .call_raw(id, "raw_return_len", (-1i32).to_le_bytes(), LIMIT)
        .expect_err("negative return length should be rejected");

    let Error::MemoryAccessOutOfBounds { len, .. } = err else {
        panic!("unexpected error: {err}");
    };
    assert_eq!(len, u32::MAX as usize);
    assert_eq!(read_counter(&mut session, id)?, 72);

    Ok(())
}

#[test]
fn top_level_oversized_return_len_is_rejected() -> Result<(), Error> {
    let (mut session, id) = deploy_badreturn()?;
    assert_eq!(read_counter(&mut session, id)?, 72);

    let oversized =
        i32::try_from(ARGBUF_LEN + 1).expect("ARGBUF_LEN fits in i32");
    let err = session
        .call_raw(id, "raw_return_len", oversized.to_le_bytes(), LIMIT)
        .expect_err("oversized return length should be rejected");

    let Error::MemoryAccessOutOfBounds { len, .. } = err else {
        panic!("unexpected error: {err}");
    };
    assert_eq!(len, ARGBUF_LEN + 1);
    assert_eq!(read_counter(&mut session, id)?, 72);

    Ok(())
}
