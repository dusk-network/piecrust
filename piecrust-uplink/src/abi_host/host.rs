// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use rkyv::AlignedVec;

use super::externs;

fn abi_len(len: usize, context: &str) -> u32 {
    u32::try_from(len).unwrap_or_else(|_| panic!("{context} exceeds u32::MAX"))
}

/// Returns the exact byte length of the active contract call input.
pub fn call_input_len() -> usize {
    unsafe { externs::call_input_len() as usize }
}

/// Copies a range of the active call input into `destination`.
///
/// The host traps if the requested source range is outside the input.
pub fn copy_call_input(input_offset: usize, destination: &mut [u8]) {
    let len = abi_len(destination.len(), "call input copy length");
    unsafe {
        externs::call_input_copy(destination.as_mut_ptr(), input_offset, len);
    }
}

/// Copies the complete call input into dynamically allocated aligned memory.
pub fn read_call_input() -> AlignedVec {
    let len = call_input_len();
    let mut input = AlignedVec::with_capacity(len);
    input.resize(len, 0);
    copy_call_input(0, &mut input);
    input
}

/// Publishes `output` as the active contract call's return data.
///
/// The returned length should be returned by the exported contract method.
pub fn publish_call_output(output: &[u8]) -> u32 {
    let len = abi_len(output.len(), "call output length");
    unsafe {
        externs::call_output_set(output.as_ptr(), len);
    }
    len
}

/// Returns the exact byte length of the most recent host operation's result.
pub fn host_result_len() -> usize {
    unsafe { externs::host_result_len() as usize }
}

/// Copies a range of the most recent host result into `destination`.
///
/// The host traps if the requested source range is outside the result.
pub fn copy_host_result(result_offset: usize, destination: &mut [u8]) {
    let len = abi_len(destination.len(), "host result copy length");
    unsafe {
        externs::host_result_copy(destination.as_mut_ptr(), result_offset, len);
    }
}

/// Copies the complete result of the most recent host operation into
/// dynamically allocated aligned memory.
pub fn read_host_result() -> AlignedVec {
    let len = host_result_len();
    let mut result = AlignedVec::with_capacity(len);
    result.resize(len, 0);
    copy_host_result(0, &mut result);
    result
}

pub(crate) fn expect_host_result_len(reported_len: usize, context: &str) {
    let actual_len = host_result_len();
    assert_eq!(
        actual_len, reported_len,
        "{context}: reported result length ({reported_len}) does not match host result length ({actual_len})"
    );
}

pub(crate) fn slice_len(slice: &[u8], context: &str) -> u32 {
    abi_len(slice.len(), context)
}
