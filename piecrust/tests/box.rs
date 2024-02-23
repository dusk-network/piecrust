// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{contract_bytecode, ContractData, Error, SessionData, VM};
use rkyv::{check_archived_root, Deserialize, Infallible};

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;

#[test]
pub fn box_set_get() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let id = session.deploy(
        contract_bytecode!("box"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let value: Option<i16> = session.call(id, "get", &(), LIMIT)?.data;

    assert_eq!(value, None);

    session.call::<i16, ()>(id, "set", &0x11, LIMIT)?;

    let value = session.call::<_, Option<i16>>(id, "get", &(), LIMIT)?.data;

    assert_eq!(value, Some(0x11));

    Ok(())
}

#[test]
pub fn box_set_get_raw() -> Result<(), Error> {
    let vm = VM::ephemeral()?;

    let mut session = vm.session(SessionData::builder())?;

    let id = session.deploy(
        contract_bytecode!("box"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let value_bytes = session.call_raw(id, "get", vec![], LIMIT)?.data;
    let value = deserialize_value(&value_bytes)?;

    assert_eq!(value, None);

    let value_bytes = serialize_value(0x11)?;
    session.call_raw(id, "set", value_bytes, LIMIT)?;

    let value_bytes = session.call_raw(id, "get", vec![], LIMIT)?.data;
    let value = deserialize_value(&value_bytes)?;

    assert_eq!(value, Some(0x11));

    Ok(())
}

fn deserialize_value(bytes: &[u8]) -> Result<Option<i16>, Error> {
    let ta = check_archived_root::<Option<i16>>(bytes)?;
    let ret = ta.deserialize(&mut Infallible).expect("Infallible");
    Ok(ret)
}

fn serialize_value(value: i16) -> Result<Vec<u8>, Error> {
    Ok(rkyv::to_bytes::<_, 16>(&value)
        .map_err(|_| Error::ValidationError)?
        .to_vec())
}
