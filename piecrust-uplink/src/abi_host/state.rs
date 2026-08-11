// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use alloc::format;
use alloc::vec::Vec;

use rkyv::validation::validators::DefaultValidator;
use rkyv::{Archive, Deserialize, Infallible, Serialize, check_archived_root};
use tracing::warn;

use super::{
    HostBufSerializer, expect_host_result_len, externs, read_host_result,
    slice_len,
};
use crate::{ContractError, ContractId, SCRATCH_BUF_BYTES};

fn serialize<T>(value: &T) -> rkyv::AlignedVec
where
    T: Serialize<HostBufSerializer>,
{
    rkyv::to_bytes::<_, SCRATCH_BUF_BYTES>(value).expect("infallible")
}

/// Execute some code that the host provides under the given name.
pub fn host_query<A, Ret>(name: &str, arg: A) -> Ret
where
    A: Serialize<HostBufSerializer>,
    Ret: Archive,
    Ret::Archived: Deserialize<Ret, Infallible>
        + for<'b> bytecheck::CheckBytes<DefaultValidator<'b>>,
{
    let arg = serialize(&arg);
    let name = name.as_bytes();
    let ret_len = unsafe {
        externs::host_query(
            name.as_ptr(),
            slice_len(name, "host query name length"),
            arg.as_ptr(),
            slice_len(&arg, "host query argument length"),
        )
    } as usize;
    expect_host_result_len(ret_len, "host query");
    let result = read_host_result();

    let archived = check_archived_root::<Ret>(&result)
        .expect("host query: return bytes are not a valid rkyv archive");
    archived.deserialize(&mut Infallible).expect("Infallible")
}

/// Calls a `contract`'s `fn_name` function with the given argument `fn_arg`.
/// The contract will have `93%` of the remaining gas available to spend.
pub fn call<A, Ret>(
    contract: ContractId,
    fn_name: &str,
    fn_arg: &A,
) -> Result<Ret, ContractError>
where
    A: Serialize<HostBufSerializer>,
    Ret: Archive,
    Ret::Archived: Deserialize<Ret, Infallible>
        + for<'b> bytecheck::CheckBytes<DefaultValidator<'b>>,
{
    call_with_limit(contract, fn_name, fn_arg, 0)
}

/// Calls a `contract`'s `fn_name` function with the given argument `fn_arg`,
/// allowing it to spend the given `gas_limit`.
pub fn call_with_limit<A, Ret>(
    contract: ContractId,
    fn_name: &str,
    fn_arg: &A,
    gas_limit: u64,
) -> Result<Ret, ContractError>
where
    A: Serialize<HostBufSerializer>,
    Ret: Archive,
    Ret::Archived: Deserialize<Ret, Infallible>
        + for<'b> bytecheck::CheckBytes<DefaultValidator<'b>>,
{
    let result =
        call_raw_with_limit(contract, fn_name, &serialize(fn_arg), gas_limit)?;
    let archived = check_archived_root::<Ret>(&result).map_err(|error| {
        warn!(
            "Deserialization failed for call return value from {:?}::{:?}: {error}",
            contract, fn_name
        );
        ContractError::Panic(format!(
            "Callee return value failed validation: {error}"
        ))
    })?;
    Ok(archived.deserialize(&mut Infallible).expect("Infallible"))
}

/// Calls the function with name `fn_name` of the given `contract` using
/// `fn_arg` as argument.
pub fn call_raw(
    contract: ContractId,
    fn_name: &str,
    fn_arg: &[u8],
) -> Result<Vec<u8>, ContractError> {
    call_raw_with_limit(contract, fn_name, fn_arg, 0)
}

/// Calls the function with name `fn_name` of the given `contract` using
/// `fn_arg` as argument, allowing it to spend the given `gas_limit`.
pub fn call_raw_with_limit(
    contract: ContractId,
    fn_name: &str,
    fn_arg: &[u8],
    gas_limit: u64,
) -> Result<Vec<u8>, ContractError> {
    let fn_name = fn_name.as_bytes();
    let ret = unsafe {
        externs::call(
            contract.as_bytes().as_ptr(),
            fn_name.as_ptr(),
            slice_len(fn_name, "contract function name length"),
            fn_arg.as_ptr(),
            slice_len(fn_arg, "contract argument length"),
            gas_limit,
        )
    };
    let result = read_host_result();

    if ret < 0 {
        Err(ContractError::from_parts(ret, &result))
    } else {
        expect_host_result_len(ret as usize, "contract call");
        Ok(result.to_vec())
    }
}
