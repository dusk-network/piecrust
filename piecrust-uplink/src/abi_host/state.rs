// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use alloc::format;
use alloc::vec::Vec;
use core::ptr;

use rkyv::validation::validators::DefaultValidator;
use rkyv::{Archive, Deserialize, Infallible, Serialize, check_archived_root};
use tracing::warn;

use super::{
    HostBufSerializer, expect_host_result_len, externs, read_host_result,
    slice_len,
};
use crate::{CONTRACT_ID_BYTES, ContractError, ContractId, SCRATCH_BUF_BYTES};

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
        externs::hq_v2(
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
        externs::c_v2(
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

/// Returns data made available by the host under the given name.
pub fn meta_data<D>(name: &str) -> Option<D>
where
    D: Archive,
    D::Archived: Deserialize<D, Infallible>
        + for<'b> bytecheck::CheckBytes<DefaultValidator<'b>>,
{
    let name = name.as_bytes();
    let ret_len = unsafe {
        externs::hd_v2(name.as_ptr(), slice_len(name, "metadata name length"))
    } as usize;
    if ret_len == 0 {
        return None;
    }

    expect_host_result_len(ret_len, "metadata");
    let result = read_host_result();
    match check_archived_root::<D>(&result) {
        Err(error) => {
            warn!("Metadata deserialization failed for {name:?}: {error}");
            None
        }
        Ok(archived) => {
            Some(archived.deserialize(&mut Infallible).expect("Infallible"))
        }
    }
}

/// Return the given contract's owner, if the contract exists.
pub fn owner<const N: usize>(contract: ContractId) -> Option<[u8; N]> {
    let len = unsafe { externs::owner_v2(contract.as_bytes().as_ptr()) };
    if len == 0 {
        None
    } else {
        Some(read_owner::<N>(len, "owner"))
    }
}

/// Returns the current contract's owner.
pub fn self_owner<const N: usize>() -> [u8; N] {
    let len = unsafe { externs::owner_v2(ptr::null()) };
    read_owner::<N>(len, "self_owner")
}

fn read_owner<const N: usize>(len: i32, context: &str) -> [u8; N] {
    assert_eq!(
        len as usize, N,
        "{context}: N ({N}) does not match host owner length ({len})"
    );
    expect_host_result_len(N, context);
    let result = read_host_result();
    result
        .as_slice()
        .try_into()
        .unwrap_or_else(|_| unreachable!("owner length was checked"))
}

/// Return the current contract's id.
pub fn self_id() -> ContractId {
    unsafe { externs::self_id_v2() };
    expect_host_result_len(CONTRACT_ID_BYTES, "self_id");
    let result = read_host_result();
    let bytes = result
        .as_slice()
        .try_into()
        .expect("self_id: host result should contain one contract ID");
    ContractId::from_bytes(bytes)
}

/// Returns the ID of the calling contract, or `None` for a root call.
pub fn caller() -> Option<ContractId> {
    match unsafe { externs::caller_v2() } {
        0 => None,
        _ => {
            expect_host_result_len(CONTRACT_ID_BYTES, "caller");
            let result = read_host_result();
            let bytes = result
                .as_slice()
                .try_into()
                .expect("caller: host result should contain one contract ID");
            Some(ContractId::from_bytes(bytes))
        }
    }
}

/// Returns IDs of all calling contracts present in the calling stack.
///
/// The current contract is not included. Index 0 is the immediate caller, and
/// the last element is the root contract call. Direct/root calls return an
/// empty stack.
pub fn callstack() -> Vec<ContractId> {
    let count = unsafe { externs::callstack_v2() } as usize;
    let expected_len = count
        .checked_mul(CONTRACT_ID_BYTES)
        .expect("callstack result length should not overflow");
    expect_host_result_len(expected_len, "callstack");
    let result = read_host_result();

    result
        .chunks_exact(CONTRACT_ID_BYTES)
        .map(|chunk| {
            let bytes = chunk
                .try_into()
                .expect("callstack chunks have contract ID length");
            ContractId::from_bytes(bytes)
        })
        .collect()
}

/// Returns the gas limit with which the contract was called.
pub fn limit() -> u64 {
    unsafe { externs::limit() }
}

/// Returns the amount of gas the contract has spent.
pub fn spent() -> u64 {
    unsafe { externs::spent() }
}

/// Emits an event with the given data, serializing it using [`rkyv`].
pub fn emit<D>(topic: &str, data: D)
where
    D: Serialize<HostBufSerializer>,
{
    emit_raw(topic, serialize(&data));
}

/// Emits an event with the given data.
pub fn emit_raw(topic: &str, data: impl AsRef<[u8]>) {
    let topic = topic.as_bytes();
    let data = data.as_ref();
    unsafe {
        externs::emit_v2(
            topic.as_ptr(),
            slice_len(topic, "event topic length"),
            data.as_ptr(),
            slice_len(data, "event data length"),
        )
    }
}

/// Feeds the host with data, serializing it using [`rkyv`].
pub fn feed<D>(data: D)
where
    D: Serialize<HostBufSerializer>,
{
    feed_raw(serialize(&data));
}

/// Feeds the host with data.
pub fn feed_raw(data: impl AsRef<[u8]>) {
    let data = data.as_ref();
    unsafe {
        externs::feed_v2(data.as_ptr(), slice_len(data, "feed data length"))
    }
}
