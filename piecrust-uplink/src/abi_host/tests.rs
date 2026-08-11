// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use alloc::vec;
use alloc::vec::Vec;
use core::slice;
use std::sync::Mutex;

use rkyv::validation::validators::DefaultValidator;
use rkyv::{AlignedVec, Deserialize, Infallible, check_archived_root};

use super::*;
use crate::{ContractId, SCRATCH_BUF_BYTES};

struct MockHost {
    input: Vec<u8>,
    output: Vec<u8>,
    result: Vec<u8>,
    calls: usize,
    queries: usize,
}

static HOST: Mutex<MockHost> = Mutex::new(MockHost {
    input: Vec::new(),
    output: Vec::new(),
    result: Vec::new(),
    calls: 0,
    queries: 0,
});

unsafe fn read_pointer(source: *const u8, len: u32) -> Vec<u8> {
    unsafe { slice::from_raw_parts(source, len as usize) }.to_vec()
}

#[unsafe(export_name = "__piecrust_b_call_input_len")]
extern "C" fn mock_call_input_len() -> u32 {
    HOST.lock().unwrap().input.len() as u32
}

#[unsafe(export_name = "__piecrust_b_call_input_copy")]
unsafe extern "C" fn mock_call_input_copy(
    destination: *mut u8,
    input_offset: usize,
    len: u32,
) -> i32 {
    let len = len as usize;
    let host = HOST.lock().unwrap();
    let source = &host.input[input_offset..input_offset + len];
    unsafe {
        destination.copy_from_nonoverlapping(source.as_ptr(), len);
    }
    len as i32
}

#[unsafe(export_name = "__piecrust_b_call_output_set")]
unsafe extern "C" fn mock_call_output_set(source: *const u8, len: u32) -> i32 {
    HOST.lock().unwrap().output = unsafe { read_pointer(source, len) };
    len as i32
}

#[unsafe(export_name = "__piecrust_b_host_result_len")]
extern "C" fn mock_host_result_len() -> u32 {
    HOST.lock().unwrap().result.len() as u32
}

#[unsafe(export_name = "__piecrust_b_host_result_copy")]
unsafe extern "C" fn mock_host_result_copy(
    destination: *mut u8,
    result_offset: usize,
    len: u32,
) -> i32 {
    let len = len as usize;
    let host = HOST.lock().unwrap();
    let source = &host.result[result_offset..result_offset + len];
    unsafe {
        destination.copy_from_nonoverlapping(source.as_ptr(), len);
    }
    len as i32
}

#[unsafe(export_name = "__piecrust_b_host_query")]
unsafe extern "C" fn mock_host_query(
    _name: *const u8,
    _name_len: u32,
    arg: *const u8,
    arg_len: u32,
) -> u32 {
    let result = unsafe { read_pointer(arg, arg_len) };
    let len = result.len() as u32;
    let mut host = HOST.lock().unwrap();
    host.result = result;
    host.queries += 1;
    len
}

#[unsafe(export_name = "__piecrust_b_call")]
unsafe extern "C" fn mock_call(
    _contract_id: *const u8,
    _fn_name: *const u8,
    _fn_name_len: u32,
    fn_arg: *const u8,
    fn_arg_len: u32,
    _gas_limit: u64,
) -> i32 {
    let result = unsafe { read_pointer(fn_arg, fn_arg_len) };
    let len = result.len() as i32;
    let mut host = HOST.lock().unwrap();
    host.result = result;
    host.calls += 1;
    len
}

fn deserialize<T>(bytes: &[u8]) -> T
where
    T: rkyv::Archive,
    T::Archived: Deserialize<T, Infallible>
        + for<'a> bytecheck::CheckBytes<DefaultValidator<'a>>,
{
    let mut aligned = AlignedVec::with_capacity(bytes.len());
    aligned.extend_from_slice(bytes);
    check_archived_root::<T>(&aligned)
        .unwrap()
        .deserialize(&mut Infallible)
        .unwrap()
}

#[test]
fn calls_and_queries_are_host_backed_for_every_payload_size() {
    let argument = vec![0x5a; 96 * 1024];
    HOST.lock().unwrap().input =
        rkyv::to_bytes::<_, SCRATCH_BUF_BYTES>(&argument)
            .unwrap()
            .to_vec();

    let output_len = wrap_call(1, |mut value: Vec<u8>| {
        value.extend_from_slice(&[1, 2, 3, 4]);
        value
    });
    let output = HOST.lock().unwrap().output.clone();
    assert_eq!(output_len as usize, output.len());
    let output: Vec<u8> = deserialize(&output);
    assert_eq!(output.len(), argument.len() + 4);

    let small = vec![0x31; 32];
    let large = vec![0x6b; 80 * 1024];
    let id = ContractId::from_bytes([7; 32]);
    assert_eq!(call_raw(id, "echo", &small).unwrap(), small);
    assert_eq!(call_raw(id, "echo", &large).unwrap(), large);

    let small_query: Vec<u8> = host_query("echo", vec![0x41u8; 32]);
    let large_query: Vec<u8> = host_query("echo", argument.clone());
    assert_eq!(small_query, vec![0x41u8; 32]);
    assert_eq!(large_query, argument);

    let host = HOST.lock().unwrap();
    assert_eq!(host.calls, 2);
    assert_eq!(host.queries, 2);
}
