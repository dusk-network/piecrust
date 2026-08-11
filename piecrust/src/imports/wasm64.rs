// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use dusk_wasmtime::{Caller, Result as WasmtimeResult};

use crate::imports;
use crate::instance::Env;

pub(crate) fn call_input_copy(
    fenv: Caller<Env>,
    guest_offset: u64,
    input_offset: u64,
    len: u32,
) -> WasmtimeResult<i32> {
    imports::call_input_copy(
        fenv,
        guest_offset as usize,
        input_offset as usize,
        len,
    )
}

pub(crate) fn call_output_set(
    fenv: Caller<Env>,
    guest_offset: u64,
    len: u32,
) -> WasmtimeResult<i32> {
    imports::call_output_set(fenv, guest_offset as usize, len)
}

pub(crate) fn host_result_copy(
    fenv: Caller<Env>,
    guest_offset: u64,
    result_offset: u64,
    len: u32,
) -> WasmtimeResult<i32> {
    imports::host_result_copy(
        fenv,
        guest_offset as usize,
        result_offset as usize,
        len,
    )
}

pub(crate) fn hq(
    fenv: Caller<Env>,
    name_ofs: u64,
    name_len: u32,
    arg_len: u32,
) -> WasmtimeResult<u32> {
    imports::hq(fenv, name_ofs as usize, name_len, arg_len)
}

pub(crate) fn hq_host(
    fenv: Caller<Env>,
    name_ofs: u64,
    name_len: u32,
    arg_ofs: u64,
    arg_len: u32,
) -> WasmtimeResult<u32> {
    imports::hq_host(
        fenv,
        name_ofs as usize,
        name_len,
        arg_ofs as usize,
        arg_len,
    )
}

pub(crate) fn hd(
    fenv: Caller<Env>,
    name_ofs: u64,
    name_len: u32,
) -> WasmtimeResult<u32> {
    imports::hd(fenv, name_ofs as usize, name_len)
}

pub(crate) fn c(
    fenv: Caller<Env>,
    mod_id_ofs: u64,
    name_ofs: u64,
    name_len: u32,
    arg_len: u32,
    gas_limit: u64,
) -> WasmtimeResult<i32> {
    imports::c(
        fenv,
        mod_id_ofs as usize,
        name_ofs as usize,
        name_len,
        arg_len,
        gas_limit,
    )
}

pub(crate) fn c_host(
    fenv: Caller<Env>,
    mod_id_ofs: u64,
    name_ofs: u64,
    name_len: u32,
    arg_ofs: u64,
    arg_len: u32,
    gas_limit: u64,
) -> WasmtimeResult<i32> {
    imports::c_host(
        fenv,
        mod_id_ofs as usize,
        name_ofs as usize,
        name_len,
        arg_ofs as usize,
        arg_len,
        gas_limit,
    )
}

pub(crate) fn emit(
    fenv: Caller<Env>,
    topic_ofs: u64,
    topic_len: u32,
    arg_len: u32,
) -> WasmtimeResult<()> {
    imports::emit(fenv, topic_ofs as usize, topic_len, arg_len)
}

pub(crate) fn owner(fenv: Caller<Env>, mod_id_ofs: u64) -> WasmtimeResult<i32> {
    imports::owner(fenv, mod_id_ofs as usize)
}
