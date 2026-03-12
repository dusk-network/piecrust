// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Tests that `check_archived_root` validation in `piecrust-uplink` correctly
//! rejects malformed return data from contracts.
//!
//! Normal-path tests (proving that valid data deserializes correctly) are
//! covered by the existing contract-specific test files (callcenter, host,
//! everest, metadata).  
//!
//! These tests focus on the *rejection* path: a malicious or buggy callee
//! writes garbage bytes into the argbuf, and the caller must receive `Err(ContractError)`
//! instead of panicking or UB.

use piecrust::{ContractData, Error, SessionData, VM, contract_bytecode};
use piecrust_uplink::ContractError;

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;

/// Inter-contract call path: a caller (`callcenter`) invokes a callee
/// (`badreturn`) whose argbuf bytes are copied one to one without host-side
/// validation.
///
/// - Garbage return bytes must produce `Err(ContractError::Panic(_))`.
/// - Valid return bytes must be deserialized correctly.
///
/// Use `bool` as the return type because rkyv has validation
/// constraints for bools (only 0 or 1 are valid).
#[test]
fn inter_contract_call_validates_return_bytes() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (badreturn_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("badreturn"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    let (center_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("callcenter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    // Garbage bytes returned by calle should produce Err(ContractError::Panic)
    //
    // badreturn writes 0xDEADBEEF... into the argbuf, bypassing wrap_call.
    let garbage_result: Result<bool, ContractError> = session
        .call(
            center_id,
            "try_query_bool",
            &(badreturn_id, String::from("garbage_value")),
            LIMIT,
        )?
        .data;

    let panic_msg = match &garbage_result {
        Err(ContractError::Panic(msg)) => msg,
        _ => panic!(
            "Garbage return bytes should produce Err(Panic), got: \
             {garbage_result:?}"
        ),
    };
    assert!(
        panic_msg.starts_with("Callee return value failed validation:"),
        "Unexpected panic error message for invalid callee bytes: {panic_msg}"
    );

    // Valid bytes returned by callee should deserialize correctly.
    //
    // badreturn serializes `true` correctly via wrap_call.
    let valid_result: Result<bool, ContractError> = session
        .call(
            center_id,
            "try_query_bool",
            &(badreturn_id, String::from("valid_bool")),
            LIMIT,
        )?
        .data;

    assert_eq!(
        valid_result,
        Ok(true),
        "Valid return bytes should deserialize correctly"
    );

    Ok(())
}

/// Host-side call path: the host calls a contract that returns garbage
/// bytes directly.  The host's `check_archived_root` (in
/// `CallReceipt::deserialize`) should reject the invalid bool byte.
#[test]
fn host_call_rejects_garbage_return() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let (badreturn_id, _) = session.deploy::<_, (), _>(
        contract_bytecode!("badreturn"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    let result =
        session.call::<_, bool>(badreturn_id, "garbage_value", &(), LIMIT);

    let err = result
        .expect_err("Host call to a contract returning garbage should fail");
    assert!(
        matches!(err, Error::ValidationError),
        "Host call to a contract returning garbage should produce a ValidationError, got: {err:?}"
    );
    assert_eq!(
        err.to_string(),
        "ValidationError",
        "Unexpected error message for host-side validation failure"
    );

    Ok(())
}
