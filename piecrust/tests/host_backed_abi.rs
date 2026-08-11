// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::sync::mpsc;

use piecrust::{
    ContractData, ContractId, Error, HOST_CALL_FRAME_MAX_LEN, SessionData, VM,
};
use wasm_encoder::{
    CodeSection, ConstExpr, DataSection, EntityType, ExportKind, ExportSection,
    Function, FunctionSection, GlobalSection, GlobalType, ImportSection,
    MemArg, MemorySection, MemoryType, Module, TypeSection, ValType,
};

const LIMIT: u64 = 100_000_000;
const OWNER: [u8; 32] = [0xabu8; 32];
const LARGE_PAYLOAD_LEN: usize = 96 * 1024;
const HOST_QUERY_PAYLOAD_LEN: usize = 296 * 1024;
const CHUNK_LEN: usize = 4 * 1024;
const RESULT_OFFSET: i32 = 8 * 1024;
const FORWARD_RESULT_OFFSET: i32 = 5 * 64 * 1024;
const QUERY_NAME_OFFSET: i32 = 5 * 64 * 1024;

fn memory_type(pages: u64) -> MemoryType {
    MemoryType {
        minimum: pages,
        maximum: Some(pages),
        memory64: false,
        shared: false,
        page_size_log2: None,
    }
}

fn marker_global() -> GlobalType {
    GlobalType {
        val_type: ValType::I32,
        mutable: false,
        shared: false,
    }
}

fn host_backed_reader(payload_len: usize) -> Vec<u8> {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], [ValType::I32]);
    types
        .ty()
        .function([ValType::I32, ValType::I32, ValType::I32], [ValType::I32]);
    types
        .ty()
        .function([ValType::I32, ValType::I32], [ValType::I32]);
    types.ty().function([ValType::I32], [ValType::I32]);
    module.section(&types);

    let mut imports = ImportSection::new();
    imports.import(
        "env",
        "__piecrust_b_call_input_len",
        EntityType::Function(0),
    );
    imports.import(
        "env",
        "__piecrust_b_call_input_copy",
        EntityType::Function(1),
    );
    imports.import(
        "env",
        "__piecrust_b_call_output_set",
        EntityType::Function(2),
    );
    module.section(&imports);

    let mut functions = FunctionSection::new();
    functions.function(3);
    module.section(&functions);
    let mut memories = MemorySection::new();
    memories.memory(memory_type(1));
    module.section(&memories);
    let mut globals = GlobalSection::new();
    globals.global(marker_global(), &ConstExpr::i32_const(0));
    module.section(&globals);
    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("B", ExportKind::Global, 0);
    exports.export("run", ExportKind::Func, 3);
    module.section(&exports);

    let mut code = CodeSection::new();
    let mut run = Function::new([]);
    run.instructions()
        .call(0)
        .local_get(0)
        .i32_ne()
        .if_(wasm_encoder::BlockType::Empty)
        .unreachable()
        .end();
    for (chunk_index, source_offset) in
        (0..payload_len).step_by(CHUNK_LEN).enumerate()
    {
        let len = CHUNK_LEN.min(payload_len - source_offset);
        run.instructions()
            .i32_const(0)
            .i32_const(source_offset as i32)
            .i32_const(len as i32)
            .call(1)
            .drop()
            .i32_const(RESULT_OFFSET + chunk_index as i32)
            .i32_const(0)
            .i32_load8_u(MemArg {
                offset: 0,
                align: 0,
                memory_index: 0,
            })
            .i32_store8(MemArg {
                offset: 0,
                align: 0,
                memory_index: 0,
            });
    }
    let result_len = payload_len.div_ceil(CHUNK_LEN) as i32;
    run.instructions()
        .i32_const(RESULT_OFFSET)
        .i32_const(result_len)
        .call(2)
        .drop()
        .i32_const(result_len)
        .end();
    code.function(&run);
    module.section(&code);
    module.finish()
}

fn host_backed_echo() -> Vec<u8> {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types
        .ty()
        .function([ValType::I32, ValType::I32, ValType::I32], [ValType::I32]);
    types
        .ty()
        .function([ValType::I32, ValType::I32], [ValType::I32]);
    types.ty().function([ValType::I32], [ValType::I32]);
    module.section(&types);
    let mut imports = ImportSection::new();
    imports.import(
        "env",
        "__piecrust_b_call_input_copy",
        EntityType::Function(0),
    );
    imports.import(
        "env",
        "__piecrust_b_call_output_set",
        EntityType::Function(1),
    );
    module.section(&imports);
    let mut functions = FunctionSection::new();
    functions.function(2);
    module.section(&functions);
    let mut memories = MemorySection::new();
    memories.memory(memory_type(1));
    module.section(&memories);
    let mut globals = GlobalSection::new();
    globals.global(marker_global(), &ConstExpr::i32_const(0));
    module.section(&globals);
    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("B", ExportKind::Global, 0);
    exports.export("run", ExportKind::Func, 2);
    module.section(&exports);
    let mut code = CodeSection::new();
    let mut run = Function::new([]);
    run.instructions()
        .i32_const(0)
        .i32_const(0)
        .local_get(0)
        .call(0)
        .drop()
        .i32_const(0)
        .local_get(0)
        .call(1)
        .drop()
        .local_get(0)
        .end();
    code.function(&run);
    module.section(&code);
    module.finish()
}

fn host_backed_feed() -> Vec<u8> {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types
        .ty()
        .function([ValType::I32, ValType::I32, ValType::I32], [ValType::I32]);
    types.ty().function([ValType::I32, ValType::I32], []);
    types
        .ty()
        .function([ValType::I32, ValType::I32], [ValType::I32]);
    types.ty().function([ValType::I32], [ValType::I32]);
    module.section(&types);
    let mut imports = ImportSection::new();
    imports.import(
        "env",
        "__piecrust_b_call_input_copy",
        EntityType::Function(0),
    );
    imports.import("env", "__piecrust_b_feed", EntityType::Function(1));
    imports.import(
        "env",
        "__piecrust_b_call_output_set",
        EntityType::Function(2),
    );
    module.section(&imports);
    let mut functions = FunctionSection::new();
    functions.function(3);
    module.section(&functions);
    let mut memories = MemorySection::new();
    memories.memory(memory_type(1));
    module.section(&memories);
    let mut globals = GlobalSection::new();
    globals.global(marker_global(), &ConstExpr::i32_const(0));
    module.section(&globals);
    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("B", ExportKind::Global, 0);
    exports.export("run", ExportKind::Func, 3);
    module.section(&exports);
    let mut code = CodeSection::new();
    let mut run = Function::new([]);
    run.instructions()
        .i32_const(0)
        .i32_const(0)
        .local_get(0)
        .call(0)
        .drop()
        .i32_const(0)
        .local_get(0)
        .call(1)
        .i32_const(0)
        .i32_const(0)
        .call(2)
        .drop()
        .i32_const(0)
        .end();
    code.function(&run);
    module.section(&code);
    module.finish()
}

fn host_backed_import_parity() -> Vec<u8> {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], [ValType::I32]);
    types
        .ty()
        .function([ValType::I32, ValType::I32, ValType::I32], [ValType::I32]);
    types
        .ty()
        .function([ValType::I32, ValType::I32], [ValType::I32]);
    types.ty().function(
        [
            ValType::I32,
            ValType::I32,
            ValType::I32,
            ValType::I32,
            ValType::I32,
            ValType::I64,
        ],
        [ValType::I32],
    );
    types.ty().function(
        [ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        [ValType::I32],
    );
    types
        .ty()
        .function([ValType::I32, ValType::I32, ValType::I32, ValType::I32], []);
    types.ty().function([ValType::I32, ValType::I32], []);
    types.ty().function([], [ValType::I64]);
    types.ty().function([ValType::I32], [ValType::I32]);
    types.ty().function([], []);
    types.ty().function([ValType::I32], [ValType::I32]);
    module.section(&types);

    let mut imports = ImportSection::new();
    imports.import(
        "env",
        "__piecrust_b_call_input_len",
        EntityType::Function(0),
    );
    imports.import(
        "env",
        "__piecrust_b_call_input_copy",
        EntityType::Function(1),
    );
    imports.import(
        "env",
        "__piecrust_b_call_output_set",
        EntityType::Function(2),
    );
    imports.import(
        "env",
        "__piecrust_b_host_result_len",
        EntityType::Function(0),
    );
    imports.import(
        "env",
        "__piecrust_b_host_result_copy",
        EntityType::Function(1),
    );
    imports.import("env", "__piecrust_b_caller", EntityType::Function(0));
    imports.import("env", "__piecrust_b_callstack", EntityType::Function(0));
    imports.import("env", "__piecrust_b_call", EntityType::Function(3));
    imports.import("env", "__piecrust_b_host_query", EntityType::Function(4));
    imports.import("env", "__piecrust_b_host_data", EntityType::Function(2));
    imports.import("env", "__piecrust_b_emit", EntityType::Function(5));
    imports.import("env", "__piecrust_b_feed", EntityType::Function(6));
    imports.import("env", "limit", EntityType::Function(7));
    imports.import("env", "spent", EntityType::Function(7));
    imports.import("env", "__piecrust_b_panic", EntityType::Function(6));
    imports.import("env", "__piecrust_b_owner", EntityType::Function(8));
    imports.import("env", "__piecrust_b_self_id", EntityType::Function(9));
    #[cfg(feature = "debug")]
    imports.import("env", "__piecrust_b_debug", EntityType::Function(6));
    module.section(&imports);
    let mut functions = FunctionSection::new();
    functions.function(10);
    module.section(&functions);
    let mut memories = MemorySection::new();
    memories.memory(memory_type(1));
    module.section(&memories);
    let mut globals = GlobalSection::new();
    globals.global(marker_global(), &ConstExpr::i32_const(0));
    module.section(&globals);
    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("B", ExportKind::Global, 0);
    exports.export(
        "run",
        ExportKind::Func,
        17 + u32::from(cfg!(feature = "debug")),
    );
    module.section(&exports);
    let mut code = CodeSection::new();
    let mut run = Function::new([]);
    run.instructions().i32_const(0).end();
    code.function(&run);
    module.section(&code);
    module.finish()
}

fn host_backed_query(name: &str) -> Vec<u8> {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], [ValType::I32]);
    types
        .ty()
        .function([ValType::I32, ValType::I32, ValType::I32], [ValType::I32]);
    types
        .ty()
        .function([ValType::I32, ValType::I32], [ValType::I32]);
    types.ty().function(
        [ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        [ValType::I32],
    );
    types.ty().function([ValType::I32], [ValType::I32]);
    module.section(&types);

    let mut imports = ImportSection::new();
    imports.import(
        "env",
        "__piecrust_b_call_input_copy",
        EntityType::Function(1),
    );
    imports.import("env", "__piecrust_b_host_query", EntityType::Function(3));
    imports.import(
        "env",
        "__piecrust_b_host_result_len",
        EntityType::Function(0),
    );
    imports.import(
        "env",
        "__piecrust_b_host_result_copy",
        EntityType::Function(1),
    );
    imports.import(
        "env",
        "__piecrust_b_call_output_set",
        EntityType::Function(2),
    );
    module.section(&imports);

    let mut functions = FunctionSection::new();
    functions.function(4);
    module.section(&functions);
    let mut memories = MemorySection::new();
    memories.memory(memory_type(6));
    module.section(&memories);
    let mut globals = GlobalSection::new();
    globals.global(marker_global(), &ConstExpr::i32_const(0));
    module.section(&globals);
    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("B", ExportKind::Global, 0);
    exports.export("run", ExportKind::Func, 5);
    module.section(&exports);

    let mut code = CodeSection::new();
    let mut run = Function::new([(1, ValType::I32)]);
    run.instructions()
        .i32_const(0)
        .i32_const(0)
        .local_get(0)
        .call(0)
        .drop()
        .i32_const(QUERY_NAME_OFFSET)
        .i32_const(name.len() as i32)
        .i32_const(0)
        .local_get(0)
        .call(1)
        .local_set(1)
        .call(2)
        .local_get(1)
        .i32_ne()
        .if_(wasm_encoder::BlockType::Empty)
        .unreachable()
        .end()
        .i32_const(0)
        .i32_const(0)
        .local_get(1)
        .call(3)
        .drop()
        .i32_const(0)
        .local_get(1)
        .call(4)
        .drop()
        .local_get(1)
        .end();
    code.function(&run);
    module.section(&code);

    let mut data = DataSection::new();
    data.active(
        0,
        &ConstExpr::i32_const(QUERY_NAME_OFFSET),
        name.as_bytes().to_vec(),
    );
    module.section(&data);
    module.finish()
}

fn host_backed_forwarder(callee: ContractId, method: &str) -> Vec<u8> {
    let target_offset = 4 * 64 * 1024;
    let method_offset = target_offset + 32;
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types
        .ty()
        .function([ValType::I32, ValType::I32, ValType::I32], [ValType::I32]);
    types.ty().function(
        [
            ValType::I32,
            ValType::I32,
            ValType::I32,
            ValType::I32,
            ValType::I32,
            ValType::I64,
        ],
        [ValType::I32],
    );
    types.ty().function([], [ValType::I32]);
    types
        .ty()
        .function([ValType::I32, ValType::I32], [ValType::I32]);
    types.ty().function([ValType::I32], [ValType::I32]);
    module.section(&types);
    let mut imports = ImportSection::new();
    imports.import(
        "env",
        "__piecrust_b_call_input_copy",
        EntityType::Function(0),
    );
    imports.import("env", "__piecrust_b_call", EntityType::Function(1));
    imports.import(
        "env",
        "__piecrust_b_host_result_len",
        EntityType::Function(2),
    );
    imports.import(
        "env",
        "__piecrust_b_host_result_copy",
        EntityType::Function(0),
    );
    imports.import(
        "env",
        "__piecrust_b_call_output_set",
        EntityType::Function(3),
    );
    module.section(&imports);
    let mut functions = FunctionSection::new();
    functions.function(4);
    functions.function(4);
    module.section(&functions);
    let mut memories = MemorySection::new();
    memories.memory(memory_type(6));
    module.section(&memories);
    let mut globals = GlobalSection::new();
    globals.global(marker_global(), &ConstExpr::i32_const(0));
    module.section(&globals);
    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("B", ExportKind::Global, 0);
    exports.export("run", ExportKind::Func, 5);
    exports.export("leaf", ExportKind::Func, 6);
    module.section(&exports);

    let mut code = CodeSection::new();
    let mut run = Function::new([(1, ValType::I32)]);
    run.instructions()
        .i32_const(0)
        .i32_const(0)
        .local_get(0)
        .call(0)
        .drop()
        .i32_const(target_offset)
        .i32_const(method_offset)
        .i32_const(method.len() as i32)
        .i32_const(0)
        .local_get(0)
        .i64_const(0)
        .call(1)
        .local_set(1)
        .local_get(1)
        .i32_const(0)
        .i32_lt_s()
        .if_(wasm_encoder::BlockType::Empty)
        .unreachable()
        .end()
        .call(2)
        .local_get(1)
        .i32_ne()
        .if_(wasm_encoder::BlockType::Empty)
        .unreachable()
        .end()
        .i32_const(FORWARD_RESULT_OFFSET)
        .i32_const(0)
        .local_get(1)
        .call(3)
        .drop()
        .i32_const(FORWARD_RESULT_OFFSET)
        .local_get(1)
        .call(4)
        .drop()
        .local_get(1)
        .end();
    code.function(&run);

    let mut leaf = Function::new([]);
    leaf.instructions()
        .i32_const(0)
        .i32_const(0)
        .local_get(0)
        .call(0)
        .drop()
        .i32_const(0)
        .local_get(0)
        .call(4)
        .drop()
        .local_get(0)
        .end();
    code.function(&leaf);
    module.section(&code);
    let mut data = DataSection::new();
    data.active(
        0,
        &ConstExpr::i32_const(target_offset),
        callee.as_bytes().to_vec(),
    );
    data.active(
        0,
        &ConstExpr::i32_const(method_offset),
        method.as_bytes().to_vec(),
    );
    module.section(&data);
    module.finish()
}

fn legacy_echo_with_extra_b_marker(extra_b_marker: bool) -> Vec<u8> {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([ValType::I32], [ValType::I32]);
    module.section(&types);
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    let mut memories = MemorySection::new();
    memories.memory(memory_type(2));
    module.section(&memories);
    let mut globals = GlobalSection::new();
    globals.global(marker_global(), &ConstExpr::i32_const(0));
    if extra_b_marker {
        globals.global(marker_global(), &ConstExpr::i32_const(0));
    }
    module.section(&globals);
    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("A", ExportKind::Global, 0);
    if extra_b_marker {
        exports.export("B", ExportKind::Global, 1);
    }
    exports.export("run", ExportKind::Func, 0);
    module.section(&exports);
    let mut code = CodeSection::new();
    let mut run = Function::new([]);
    run.instructions().local_get(0).end();
    code.function(&run);
    module.section(&code);
    module.finish()
}

fn legacy_echo() -> Vec<u8> {
    legacy_echo_with_extra_b_marker(false)
}

fn payload(len: usize) -> Vec<u8> {
    let mut payload = vec![0u8; len];
    for (chunk_index, chunk) in payload.chunks_mut(CHUNK_LEN).enumerate() {
        chunk.fill(chunk_index as u8);
    }
    payload
}

fn expected_chunk_markers(len: usize) -> Vec<u8> {
    (0..len.div_ceil(CHUNK_LEN))
        .map(|index| index as u8)
        .collect()
}

fn enabled_session(vm: &VM) -> Result<piecrust::Session, Error> {
    vm.session(SessionData::builder().host_backed_abi_enabled(true))
}

#[test]
fn host_backed_frames_are_bounded_gated_and_copy_metered() -> Result<(), Error>
{
    let vm = VM::ephemeral()?;
    let mut disabled = vm.session(SessionData::builder())?;
    let err = disabled
        .deploy::<_, (), _>(
            &host_backed_echo(),
            ContractData::builder().owner(OWNER),
            LIMIT,
        )
        .expect_err("B must be session gated");
    assert!(matches!(err, Error::HostBackedAbiNotEnabled));

    let legacy = ContractId::from_bytes([0x40; 32]);
    disabled.deploy::<_, (), _>(
        &legacy_echo_with_extra_b_marker(true),
        ContractData::builder().contract_id(legacy).owner(OWNER),
        LIMIT,
    )?;
    let receipt =
        disabled.call_raw(legacy, "run", b"legacy".to_vec(), LIMIT)?;
    assert_eq!(receipt.data, b"legacy");

    let mut session = enabled_session(&vm)?;
    let reader = ContractId::from_bytes([0x41; 32]);
    session.deploy::<_, (), _>(
        &host_backed_reader(HOST_CALL_FRAME_MAX_LEN),
        ContractData::builder().contract_id(reader).owner(OWNER),
        LIMIT,
    )?;
    let receipt = session.call_raw(
        reader,
        "run",
        payload(HOST_CALL_FRAME_MAX_LEN),
        LIMIT,
    )?;
    assert_eq!(
        receipt.data,
        expected_chunk_markers(HOST_CALL_FRAME_MAX_LEN)
    );
    assert_eq!(session.memory_len(reader)?, Some(64 * 1024));
    let err = session
        .call_raw(reader, "run", vec![0; HOST_CALL_FRAME_MAX_LEN + 1], LIMIT)
        .expect_err("one byte above the frame limit must fail");
    assert!(matches!(
        err,
        Error::ArgumentBufferOverflow { max_len, .. }
            if max_len == HOST_CALL_FRAME_MAX_LEN
    ));

    let echo = ContractId::from_bytes([0x42; 32]);
    session.deploy::<_, (), _>(
        &host_backed_echo(),
        ContractData::builder().contract_id(echo).owner(OWNER),
        LIMIT,
    )?;
    let short = session.call_raw(echo, "run", vec![1; 1024], LIMIT)?;
    let long = session.call_raw(echo, "run", vec![1; 2048], LIMIT)?;
    assert_eq!(long.gas_spent - short.gas_spent, 8 * 1024);
    Ok(())
}

#[test]
fn b_nested_calls_are_generic_mixed_and_reentrant() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = enabled_session(&vm)?;
    let b_callee = ContractId::from_bytes([0x51; 32]);
    let b_caller = ContractId::from_bytes([0x52; 32]);
    session.deploy::<_, (), _>(
        &host_backed_reader(LARGE_PAYLOAD_LEN),
        ContractData::builder().contract_id(b_callee).owner(OWNER),
        LIMIT,
    )?;
    session.deploy::<_, (), _>(
        &host_backed_forwarder(b_callee, "run"),
        ContractData::builder().contract_id(b_caller).owner(OWNER),
        LIMIT,
    )?;
    let receipt =
        session.call_raw(b_caller, "run", payload(LARGE_PAYLOAD_LEN), LIMIT)?;
    assert_eq!(receipt.data, expected_chunk_markers(LARGE_PAYLOAD_LEN));
    assert_eq!(
        receipt
            .call_tree
            .iter()
            .map(|frame| frame.contract_id)
            .collect::<Vec<_>>(),
        vec![b_callee, b_caller]
    );

    let legacy = ContractId::from_bytes([0x53; 32]);
    let mixed = ContractId::from_bytes([0x54; 32]);
    session.deploy::<_, (), _>(
        &legacy_echo(),
        ContractData::builder().contract_id(legacy).owner(OWNER),
        LIMIT,
    )?;
    session.deploy::<_, (), _>(
        &host_backed_forwarder(legacy, "run"),
        ContractData::builder().contract_id(mixed).owner(OWNER),
        LIMIT,
    )?;
    session
        .call_raw(mixed, "run", vec![0x7a; 64 * 1024 + 1], LIMIT)
        .expect_err("an A callee must retain its 64 KiB capacity");

    let reentrant = ContractId::from_bytes([0x55; 32]);
    session.deploy::<_, (), _>(
        &host_backed_forwarder(reentrant, "leaf"),
        ContractData::builder().contract_id(reentrant).owner(OWNER),
        LIMIT,
    )?;
    let data = vec![0x33; 1024];
    let receipt = session.call_raw(reentrant, "run", data.clone(), LIMIT)?;
    assert_eq!(receipt.data, data);
    assert_eq!(
        receipt
            .call_tree
            .iter()
            .map(|frame| frame.contract_id)
            .collect::<Vec<_>>(),
        vec![reentrant, reentrant]
    );
    Ok(())
}

#[test]
fn b_host_queries_round_trip_large_requests_and_results() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;
    vm.register_host_query("reverse", |buf: &mut [u8], len: u32| {
        buf[..len as usize].reverse();
        len
    });
    let mut session = enabled_session(&vm)?;
    let contract = ContractId::from_bytes([0x56; 32]);
    session.deploy::<_, (), _>(
        &host_backed_query("reverse"),
        ContractData::builder().contract_id(contract).owner(OWNER),
        LIMIT,
    )?;

    let input = payload(HOST_QUERY_PAYLOAD_LEN);
    let mut expected = input.clone();
    expected.reverse();
    let receipt = session.call_raw(contract, "run", input, LIMIT)?;
    assert_eq!(receipt.data, expected);
    Ok(())
}

#[test]
fn cached_b_modules_remain_session_gated() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let contract = ContractId::from_bytes([0x57; 32]);
    let mut enabled = enabled_session(&vm)?;
    enabled.deploy::<_, (), _>(
        &host_backed_echo(),
        ContractData::builder().contract_id(contract).owner(OWNER),
        LIMIT,
    )?;
    let root = enabled.commit()?;

    let mut disabled = vm.session(SessionData::builder().base(root))?;
    let error = disabled
        .call_raw(contract, "run", b"disabled".to_vec(), LIMIT)
        .expect_err("cached B modules must still require session activation");
    assert!(
        matches!(error, Error::HostBackedAbiNotEnabled),
        "unexpected disabled-session error: {error:?}"
    );

    let mut enabled = vm.session(
        SessionData::builder()
            .base(root)
            .host_backed_abi_enabled(true),
    )?;
    let receipt =
        enabled.call_raw(contract, "run", b"enabled".to_vec(), LIMIT)?;
    assert_eq!(receipt.data, b"enabled");
    Ok(())
}

#[test]
fn uplink_b_contract_round_trips_large_typed_data() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = enabled_session(&vm)?;
    let contract = ContractId::from_bytes([0x58; 32]);
    let init = payload(96 * 1024);
    session.deploy::<_, (), _>(
        piecrust::contract_bytecode!("host_backed"),
        ContractData::builder()
            .contract_id(contract)
            .owner(OWNER)
            .init_arg(&init),
        LIMIT,
    )?;

    let input = payload(HOST_QUERY_PAYLOAD_LEN);
    let receipt =
        session.call::<_, Vec<u8>>(contract, "echo", &input, LIMIT)?;
    assert_eq!(receipt.data, input);
    Ok(())
}

#[test]
fn feeder_channel_remains_available_to_b_sessions() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = enabled_session(&vm)?;
    let id = ContractId::from_bytes([0x81; 32]);
    session.deploy::<_, (), _>(
        &host_backed_feed(),
        ContractData::builder().contract_id(id).owner(OWNER),
        LIMIT,
    )?;
    session.deploy::<_, (), _>(
        &host_backed_import_parity(),
        ContractData::builder()
            .contract_id(ContractId::from_bytes([0x82; 32]))
            .owner(OWNER),
        LIMIT,
    )?;
    let (sender, receiver) = mpsc::channel();
    let receipt =
        session.feeder_call_raw(id, "run", b"frame".to_vec(), LIMIT, sender)?;
    assert!(receipt.data.is_empty());
    assert_eq!(receiver.recv().unwrap(), b"frame");
    Ok(())
}
