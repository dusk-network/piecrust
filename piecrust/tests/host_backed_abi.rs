// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};

use piecrust::{
    CommittedCall, ContractData, ContractId, Error, HOST_CALL_FRAME_MAX_LEN,
    SessionData, VM,
};
use wasm_encoder::{
    CodeSection, ConstExpr, DataSection, EntityType, ExportKind, ExportSection,
    Function, FunctionSection, GlobalSection, GlobalType, ImportSection,
    MemArg, MemorySection, MemoryType, Module, TypeSection, ValType,
};

const LIMIT: u64 = 100_000_000;
const OWNER: [u8; 32] = [0xabu8; 32];
const COMMITTED_PAYLOAD_LEN: usize = 96 * 1024;
const HOST_QUERY_PAYLOAD_LEN: usize = 296 * 1024;
const CHUNK_LEN: usize = 4 * 1024;
const RESULT_OFFSET: i32 = 8 * 1024;
const FORWARD_RESULT_OFFSET: i32 = 5 * 64 * 1024;
const QUERY_NAME_OFFSET: i32 = 5 * 64 * 1024;

fn memory_type(pages: u64) -> MemoryType {
    memory_type_with_width(pages, false)
}

fn memory_type_with_width(pages: u64, memory64: bool) -> MemoryType {
    MemoryType {
        minimum: pages,
        maximum: Some(pages),
        memory64,
        shared: false,
        page_size_log2: None,
    }
}

fn marker_global() -> GlobalType {
    marker_global_with_type(ValType::I32, false)
}

fn marker_global_with_type(val_type: ValType, mutable: bool) -> GlobalType {
    GlobalType {
        val_type,
        mutable,
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

fn host_backed_sized_output(output_len: usize) -> Vec<u8> {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types
        .ty()
        .function([ValType::I32, ValType::I32], [ValType::I32]);
    types.ty().function([ValType::I32], [ValType::I32]);
    module.section(&types);
    let mut imports = ImportSection::new();
    imports.import(
        "env",
        "__piecrust_b_call_output_set",
        EntityType::Function(0),
    );
    module.section(&imports);
    let mut functions = FunctionSection::new();
    functions.function(1);
    module.section(&functions);
    let mut memories = MemorySection::new();
    let pages = output_len.max(1).div_ceil(64 * 1024) as u64;
    memories.memory(memory_type(pages));
    module.section(&memories);
    let mut globals = GlobalSection::new();
    globals.global(marker_global(), &ConstExpr::i32_const(0));
    module.section(&globals);
    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("B", ExportKind::Global, 0);
    exports.export("run", ExportKind::Func, 1);
    module.section(&exports);

    let mut code = CodeSection::new();
    let mut run = Function::new([]);
    run.instructions()
        .i32_const(0)
        .i32_const(0)
        .i32_load8_u(MemArg {
            offset: 0,
            align: 0,
            memory_index: 0,
        })
        .i32_const(1)
        .i32_add()
        .i32_store8(MemArg {
            offset: 0,
            align: 0,
            memory_index: 0,
        })
        .i32_const(0)
        .i32_const(output_len as i32)
        .call(0)
        .drop()
        .i32_const(output_len as i32)
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
    types.ty().function([ValType::I32], []);
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
    imports.import("env", "feed", EntityType::Function(1));
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
        .function([ValType::I32, ValType::I32, ValType::I32], []);
    types.ty().function([ValType::I32], []);
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
    imports.import("env", "caller", EntityType::Function(0));
    imports.import("env", "callstack", EntityType::Function(0));
    imports.import("env", "__piecrust_b_call", EntityType::Function(3));
    imports.import("env", "__piecrust_b_host_query", EntityType::Function(4));
    imports.import("env", "hd", EntityType::Function(2));
    imports.import("env", "emit", EntityType::Function(5));
    imports.import("env", "feed", EntityType::Function(6));
    imports.import("env", "limit", EntityType::Function(7));
    imports.import("env", "spent", EntityType::Function(7));
    imports.import("env", "panic", EntityType::Function(6));
    imports.import("env", "owner", EntityType::Function(8));
    imports.import("env", "self_id", EntityType::Function(9));
    #[cfg(feature = "debug")]
    imports.import("env", "hdebug", EntityType::Function(6));
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
    memories.memory(memory_type(7));
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

fn host_backed_marker_contract(
    memory64: bool,
    marker_type: ValType,
    marker_offset: u64,
    pages: u64,
    mutable: bool,
) -> Vec<u8> {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([ValType::I32], [ValType::I32]);
    module.section(&types);
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    let mut memories = MemorySection::new();
    memories.memory(memory_type_with_width(pages, memory64));
    module.section(&memories);
    let mut globals = GlobalSection::new();
    let value = match marker_type {
        ValType::I32 => ConstExpr::i32_const(marker_offset as i32),
        ValType::I64 => ConstExpr::i64_const(marker_offset as i64),
        _ => unreachable!("marker tests only use integer globals"),
    };
    globals.global(marker_global_with_type(marker_type, mutable), &value);
    module.section(&globals);
    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("B", ExportKind::Global, 0);
    exports.export("run", ExportKind::Func, 0);
    module.section(&exports);
    let mut code = CodeSection::new();
    let mut run = Function::new([]);
    run.instructions().i32_const(0).end();
    code.function(&run);
    module.section(&code);
    module.finish()
}

fn legacy_forwarder(
    callee: ContractId,
    method: &str,
    catch_failure: bool,
) -> Vec<u8> {
    let target_offset = 64 * 1024;
    let method_offset = target_offset + 32;
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function(
        [
            ValType::I32,
            ValType::I32,
            ValType::I32,
            ValType::I32,
            ValType::I64,
        ],
        [ValType::I32],
    );
    types.ty().function([ValType::I32], [ValType::I32]);
    module.section(&types);
    let mut imports = ImportSection::new();
    imports.import("env", "c", EntityType::Function(0));
    module.section(&imports);
    let mut functions = FunctionSection::new();
    functions.function(1);
    module.section(&functions);
    let mut memories = MemorySection::new();
    memories.memory(memory_type(2));
    module.section(&memories);
    let mut globals = GlobalSection::new();
    globals.global(marker_global(), &ConstExpr::i32_const(0));
    module.section(&globals);
    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("A", ExportKind::Global, 0);
    exports.export("run", ExportKind::Func, 1);
    module.section(&exports);
    let mut code = CodeSection::new();
    let mut run = Function::new([]);
    let mut instructions = run.instructions();
    instructions
        .i32_const(target_offset)
        .i32_const(method_offset)
        .i32_const(method.len() as i32)
        .local_get(0)
        .i64_const(0)
        .call(0);
    if catch_failure {
        instructions.drop().i32_const(0);
    }
    instructions.end();
    code.function(&run);
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

fn committed_session(vm: &VM) -> Result<piecrust::Session, Error> {
    vm.session(
        SessionData::builder()
            .host_backed_abi_enabled(true)
            .committed_call_enabled(true),
    )
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
fn b_marker_is_a_width_matched_validated_scratch_offset() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = enabled_session(&vm)?;

    for (index, (memory64, marker_type)) in
        [(false, ValType::I32), (true, ValType::I64)]
            .into_iter()
            .enumerate()
    {
        let id = ContractId::from_bytes([0x90 + index as u8; 32]);
        session.deploy::<_, (), _>(
            &host_backed_marker_contract(
                memory64,
                marker_type,
                64 * 1024,
                2,
                false,
            ),
            ContractData::builder().contract_id(id).owner(OWNER),
            LIMIT,
        )?;
    }

    let invalid = [
        host_backed_marker_contract(false, ValType::I64, 0, 2, false),
        host_backed_marker_contract(true, ValType::I32, 0, 2, false),
        host_backed_marker_contract(false, ValType::I32, 1, 1, false),
        host_backed_marker_contract(false, ValType::I32, 0, 1, true),
    ];
    for (index, module) in invalid.into_iter().enumerate() {
        let id = ContractId::from_bytes([0xa0 + index as u8; 32]);
        let error = session
            .deploy::<_, (), _>(
                &module,
                ContractData::builder().contract_id(id).owner(OWNER),
                LIMIT,
            )
            .expect_err("invalid B scratch markers must be rejected");
        assert!(
            matches!(error, Error::InvalidArgumentBuffer),
            "unexpected marker error: {error:?}"
        );
    }
    Ok(())
}

#[test]
fn b_calls_keep_one_transport_for_asymmetric_payloads() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = enabled_session(&vm)?;

    let large_callee = ContractId::from_bytes([0x61; 32]);
    let small_caller = ContractId::from_bytes([0x62; 32]);
    session.deploy::<_, (), _>(
        &host_backed_sized_output(LARGE_PAYLOAD_LEN),
        ContractData::builder()
            .contract_id(large_callee)
            .owner(OWNER),
        LIMIT,
    )?;
    session.deploy::<_, (), _>(
        &host_backed_forwarder(large_callee, "run"),
        ContractData::builder()
            .contract_id(small_caller)
            .owner(OWNER),
        LIMIT,
    )?;
    let receipt =
        session.call_raw(small_caller, "run", vec![0x11; 32], LIMIT)?;
    assert_eq!(receipt.data.len(), LARGE_PAYLOAD_LEN);
    assert_eq!(receipt.data[0], 1, "callee must execute exactly once");
    assert!(receipt.data[1..].iter().all(|byte| *byte == 0));

    let small_callee = ContractId::from_bytes([0x63; 32]);
    let large_caller = ContractId::from_bytes([0x64; 32]);
    session.deploy::<_, (), _>(
        &host_backed_sized_output(32),
        ContractData::builder()
            .contract_id(small_callee)
            .owner(OWNER),
        LIMIT,
    )?;
    session.deploy::<_, (), _>(
        &host_backed_forwarder(small_callee, "run"),
        ContractData::builder()
            .contract_id(large_caller)
            .owner(OWNER),
        LIMIT,
    )?;
    let receipt = session.call_raw(
        large_caller,
        "run",
        vec![0x22; LARGE_PAYLOAD_LEN],
        LIMIT,
    )?;
    assert_eq!(receipt.data.len(), 32);
    assert_eq!(receipt.data[0], 1, "callee must execute exactly once");
    assert!(receipt.data[1..].iter().all(|byte| *byte == 0));
    Ok(())
}

#[test]
fn b_nested_calls_are_generic_mixed_and_reentrant() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = enabled_session(&vm)?;
    let b_callee = ContractId::from_bytes([0x51; 32]);
    let b_caller = ContractId::from_bytes([0x52; 32]);
    session.deploy::<_, (), _>(
        &host_backed_reader(COMMITTED_PAYLOAD_LEN),
        ContractData::builder().contract_id(b_callee).owner(OWNER),
        LIMIT,
    )?;
    session.deploy::<_, (), _>(
        &host_backed_forwarder(b_callee, "run"),
        ContractData::builder().contract_id(b_caller).owner(OWNER),
        LIMIT,
    )?;
    let receipt = session.call_raw(
        b_caller,
        "run",
        payload(COMMITTED_PAYLOAD_LEN),
        LIMIT,
    )?;
    assert_eq!(receipt.data, expected_chunk_markers(COMMITTED_PAYLOAD_LEN));
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
fn b_host_queries_keep_one_transport_for_asymmetric_payloads()
-> Result<(), Error> {
    let invocations = Arc::new(AtomicUsize::new(0));
    let query_invocations = Arc::clone(&invocations);
    let mut vm = VM::ephemeral()?;
    vm.register_host_query("resize", move |buf: &mut [u8], len: u32| {
        let invocation = query_invocations.fetch_add(1, Ordering::SeqCst) + 1;
        let output_len = if len as usize <= 64 {
            LARGE_PAYLOAD_LEN
        } else {
            32
        };
        buf[..output_len].fill(0);
        buf[0] = invocation as u8;
        output_len as u32
    });
    let mut session = enabled_session(&vm)?;
    let contract = ContractId::from_bytes([0x65; 32]);
    session.deploy::<_, (), _>(
        &host_backed_query("resize"),
        ContractData::builder().contract_id(contract).owner(OWNER),
        LIMIT,
    )?;

    let large = session.call_raw(contract, "run", vec![0x33; 32], LIMIT)?;
    assert_eq!(large.data.len(), LARGE_PAYLOAD_LEN);
    assert_eq!(large.data[0], 1);

    let small = session.call_raw(
        contract,
        "run",
        vec![0x44; LARGE_PAYLOAD_LEN],
        LIMIT,
    )?;
    assert_eq!(small.data.len(), 32);
    assert_eq!(small.data[0], 2);
    assert_eq!(invocations.load(Ordering::SeqCst), 2);
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
fn committed_payload_is_exact_one_shot_and_legacy_root_scoped()
-> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = committed_session(&vm)?;
    let callee = ContractId::from_bytes([0x61; 32]);
    let caller = ContractId::from_bytes([0x62; 32]);
    session.deploy::<_, (), _>(
        &host_backed_reader(COMMITTED_PAYLOAD_LEN),
        ContractData::builder().contract_id(callee).owner(OWNER),
        LIMIT,
    )?;
    session.deploy::<_, (), _>(
        &legacy_forwarder(callee, "run", false),
        ContractData::builder().contract_id(caller).owner(OWNER),
        LIMIT,
    )?;

    let committed =
        CommittedCall::new(callee, "run", payload(COMMITTED_PAYLOAD_LEN))?;
    let descriptor = committed.dispatch_argument().to_vec();
    let receipt = session.call_raw_with_committed_call(
        committed, caller, "run", descriptor, LIMIT,
    )?;
    assert_eq!(receipt.data, expected_chunk_markers(COMMITTED_PAYLOAD_LEN));
    assert_eq!(
        receipt
            .call_tree
            .iter()
            .map(|frame| frame.contract_id)
            .collect::<Vec<_>>(),
        vec![callee, caller]
    );

    let committed =
        CommittedCall::new(callee, "run", payload(COMMITTED_PAYLOAD_LEN))?;
    let mut wrong_descriptor = committed.dispatch_argument().to_vec();
    wrong_descriptor[0] ^= 1;
    session
        .call_raw_with_committed_call(
            committed,
            caller,
            "run",
            wrong_descriptor,
            LIMIT,
        )
        .expect_err("the descriptor must match exactly");
    let descriptor =
        CommittedCall::new(callee, "run", payload(COMMITTED_PAYLOAD_LEN))?
            .dispatch_argument()
            .to_vec();
    session
        .call_raw(caller, "run", descriptor, LIMIT)
        .expect_err("a failed context must not leak into another root call");
    Ok(())
}

#[test]
fn committed_payload_can_replace_a_compact_dispatch_method() -> Result<(), Error>
{
    let vm = VM::ephemeral()?;
    let mut session = committed_session(&vm)?;
    let callee = ContractId::from_bytes([0x63; 32]);
    let caller = ContractId::from_bytes([0x64; 32]);
    session.deploy::<_, (), _>(
        &host_backed_reader(COMMITTED_PAYLOAD_LEN),
        ContractData::builder().contract_id(callee).owner(OWNER),
        LIMIT,
    )?;
    session.deploy::<_, (), _>(
        &legacy_forwarder(callee, "committed_dispatch", false),
        ContractData::builder().contract_id(caller).owner(OWNER),
        LIMIT,
    )?;

    let codeword = vec![0xc7; 513];
    let committed =
        CommittedCall::new(callee, "run", payload(COMMITTED_PAYLOAD_LEN))?
            .dispatch_via("committed_dispatch", codeword.clone())?;
    let descriptor = committed.dispatch_argument().to_vec();
    assert_eq!(descriptor, codeword);
    let receipt = session.call_raw_with_committed_call(
        committed, caller, "run", descriptor, LIMIT,
    )?;

    assert_eq!(receipt.data, expected_chunk_markers(COMMITTED_PAYLOAD_LEN));
    assert_eq!(
        receipt
            .call_tree
            .iter()
            .map(|frame| frame.contract_id)
            .collect::<Vec<_>>(),
        vec![callee, caller]
    );
    Ok(())
}

#[test]
fn caught_committed_child_failure_resolves_the_exact_attempt()
-> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = committed_session(&vm)?;
    let callee = ContractId::from_bytes([0x71; 32]);
    let caller = ContractId::from_bytes([0x72; 32]);
    session.deploy::<_, (), _>(
        &host_backed_reader(COMMITTED_PAYLOAD_LEN),
        ContractData::builder().contract_id(callee).owner(OWNER),
        LIMIT,
    )?;
    session.deploy::<_, (), _>(
        &legacy_forwarder(callee, "missing", true),
        ContractData::builder().contract_id(caller).owner(OWNER),
        LIMIT,
    )?;
    let committed =
        CommittedCall::new(callee, "missing", payload(COMMITTED_PAYLOAD_LEN))?;
    let descriptor = committed.dispatch_argument().to_vec();
    let receipt = session.call_raw_with_committed_call(
        committed, caller, "run", descriptor, LIMIT,
    )?;
    assert_eq!(
        receipt
            .call_tree
            .iter()
            .map(|frame| frame.contract_id)
            .collect::<Vec<_>>(),
        vec![caller]
    );

    let echo = ContractId::from_bytes([0x73; 32]);
    session.deploy::<_, (), _>(
        &host_backed_echo(),
        ContractData::builder().contract_id(echo).owner(OWNER),
        LIMIT,
    )?;
    assert_eq!(
        session
            .call_raw(echo, "run", b"clean".to_vec(), LIMIT)?
            .data,
        b"clean"
    );
    Ok(())
}

#[test]
fn committed_delivery_has_an_independent_gate_and_requires_b()
-> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let callee = ContractId::from_bytes([0x74; 32]);
    let caller = ContractId::from_bytes([0x75; 32]);
    let mut host_only = enabled_session(&vm)?;
    host_only.deploy::<_, (), _>(
        &host_backed_echo(),
        ContractData::builder().contract_id(callee).owner(OWNER),
        LIMIT,
    )?;
    host_only.deploy::<_, (), _>(
        &legacy_forwarder(callee, "run", true),
        ContractData::builder().contract_id(caller).owner(OWNER),
        LIMIT,
    )?;
    let committed = CommittedCall::new(callee, "run", vec![1; 1024])?;
    let descriptor = committed.dispatch_argument().to_vec();
    assert!(matches!(
        host_only
            .call_raw_with_committed_call(
                committed, caller, "run", descriptor, LIMIT,
            )
            .expect_err("committed delivery must have its own activation gate"),
        Error::CommittedCallNotEnabled
    ));

    let mut session = committed_session(&vm)?;
    let legacy = ContractId::from_bytes([0x76; 32]);
    let legacy_caller = ContractId::from_bytes([0x77; 32]);
    session.deploy::<_, (), _>(
        &legacy_echo(),
        ContractData::builder().contract_id(legacy).owner(OWNER),
        LIMIT,
    )?;
    session.deploy::<_, (), _>(
        &legacy_forwarder(legacy, "run", true),
        ContractData::builder()
            .contract_id(legacy_caller)
            .owner(OWNER),
        LIMIT,
    )?;
    let committed = CommittedCall::new(legacy, "run", vec![2; 1024])?;
    let descriptor = committed.dispatch_argument().to_vec();
    let receipt = session.call_raw_with_committed_call(
        committed,
        legacy_caller,
        "run",
        descriptor,
        LIMIT,
    )?;
    assert_eq!(
        receipt
            .call_tree
            .iter()
            .map(|frame| frame.contract_id)
            .collect::<Vec<_>>(),
        vec![legacy_caller],
        "the legacy callee must reject hidden payload substitution"
    );
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
