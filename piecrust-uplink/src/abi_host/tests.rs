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
use crate::{CONTRACT_ID_BYTES, ContractId, SCRATCH_BUF_BYTES};

struct MockHost {
    input: Vec<u8>,
    output: Vec<u8>,
    result: Vec<u8>,
    metadata: Vec<u8>,
    event: Vec<u8>,
    feed: Vec<u8>,
}

static HOST: Mutex<MockHost> = Mutex::new(MockHost {
    input: Vec::new(),
    output: Vec::new(),
    result: Vec::new(),
    metadata: Vec::new(),
    event: Vec::new(),
    feed: Vec::new(),
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
unsafe extern "C" fn mock_hq_v2(
    _name: *const u8,
    _name_len: u32,
    arg: *const u8,
    arg_len: u32,
) -> u32 {
    let result = unsafe { read_pointer(arg, arg_len) };
    let len = result.len() as u32;
    HOST.lock().unwrap().result = result;
    len
}

#[unsafe(export_name = "__piecrust_b_host_data")]
extern "C" fn mock_hd_v2(_name: *const u8, _name_len: u32) -> u32 {
    let mut host = HOST.lock().unwrap();
    host.result = host.metadata.clone();
    host.result.len() as u32
}

#[unsafe(export_name = "__piecrust_b_call")]
unsafe extern "C" fn mock_c_v2(
    _contract_id: *const u8,
    _fn_name: *const u8,
    _fn_name_len: u32,
    fn_arg: *const u8,
    fn_arg_len: u32,
    _gas_limit: u64,
) -> i32 {
    let result = unsafe { read_pointer(fn_arg, fn_arg_len) };
    let len = result.len() as i32;
    HOST.lock().unwrap().result = result;
    len
}

#[unsafe(export_name = "__piecrust_b_emit")]
unsafe extern "C" fn mock_emit_v2(
    _topic: *const u8,
    _topic_len: u32,
    data: *const u8,
    data_len: u32,
) {
    HOST.lock().unwrap().event = unsafe { read_pointer(data, data_len) };
}

#[unsafe(export_name = "__piecrust_b_feed")]
unsafe extern "C" fn mock_feed_v2(data: *const u8, data_len: u32) {
    HOST.lock().unwrap().feed = unsafe { read_pointer(data, data_len) };
}

#[unsafe(export_name = "__piecrust_b_caller")]
extern "C" fn mock_caller_v2() -> i32 {
    HOST.lock().unwrap().result = vec![0x11; CONTRACT_ID_BYTES];
    1
}

#[unsafe(export_name = "__piecrust_b_callstack")]
extern "C" fn mock_callstack_v2() -> i32 {
    let mut result = vec![0x22; CONTRACT_ID_BYTES];
    result.extend_from_slice(&[0x33; CONTRACT_ID_BYTES]);
    HOST.lock().unwrap().result = result;
    2
}

#[unsafe(export_name = "__piecrust_b_owner")]
extern "C" fn mock_owner_v2(_contract_id: *const u8) -> i32 {
    HOST.lock().unwrap().result = vec![0x44; CONTRACT_ID_BYTES];
    CONTRACT_ID_BYTES as i32
}

#[unsafe(export_name = "__piecrust_b_self_id")]
extern "C" fn mock_self_id_v2() {
    HOST.lock().unwrap().result = vec![0x55; CONTRACT_ID_BYTES];
}

#[unsafe(export_name = "limit")]
extern "C" fn mock_limit() -> u64 {
    1_000_000
}

#[unsafe(export_name = "spent")]
extern "C" fn mock_spent() -> u64 {
    42
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
fn supports_dynamic_calls_and_pointer_host_apis() {
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
    assert_eq!(&output[..argument.len()], argument);

    let raw = vec![0x6b; 80 * 1024];
    assert_eq!(
        call_raw(ContractId::from_bytes([7; 32]), "echo", &raw).unwrap(),
        raw
    );

    let queried: Vec<u8> = host_query("echo", argument.clone());
    assert_eq!(queried, argument);

    HOST.lock().unwrap().metadata =
        rkyv::to_bytes::<_, SCRATCH_BUF_BYTES>(&argument)
            .unwrap()
            .to_vec();
    assert_eq!(meta_data::<Vec<u8>>("large"), Some(argument.clone()));

    emit_raw("large", &raw);
    feed_raw(&raw);
    let host = HOST.lock().unwrap();
    assert_eq!(host.event, raw);
    assert_eq!(host.feed, raw);
    drop(host);

    assert_eq!(caller(), Some(ContractId::from_bytes([0x11; 32])));
    assert_eq!(
        callstack(),
        vec![
            ContractId::from_bytes([0x22; 32]),
            ContractId::from_bytes([0x33; 32]),
        ]
    );
    assert_eq!(self_owner::<32>(), [0x44; 32]);
    assert_eq!(
        owner::<32>(ContractId::from_bytes([9; 32])),
        Some([0x44; 32])
    );
    assert_eq!(self_id(), ContractId::from_bytes([0x55; 32]));
    assert_eq!(limit(), 1_000_000);
    assert_eq!(spent(), 42);

    let mut first = [0; 17];
    let mut second = [0; 15];
    copy_host_result(0, &mut first);
    copy_host_result(17, &mut second);
    assert_eq!(first, [0x55; 17]);
    assert_eq!(second, [0x55; 15]);
}
