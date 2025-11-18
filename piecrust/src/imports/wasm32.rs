// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use dusk_wasmtime::{Caller, Result as WasmtimeResult};

use crate::imports;
use crate::instance::Env;

pub(crate) fn hq(
    fenv: Caller<Env>,
    name_ofs: u32,
    name_len: u32,
    arg_len: u32,
) -> WasmtimeResult<u32> {

    println!("------ IN wasm32 hq");
    imports::hq(fenv, name_ofs as usize, name_len, arg_len)
}

pub(crate) fn hd(
    fenv: Caller<Env>,
    name_ofs: u32,
    name_len: u32,
) -> WasmtimeResult<u32> {
    println!("------ IN wasm32 hd");
    imports::hd(fenv, name_ofs as usize, name_len)
}

pub(crate) fn c(
    fenv: Caller<Env>,
    mod_id_ofs: u32,
    name_ofs: u32,
    name_len: u32,
    arg_len: u32,
    gas_limit: u64,
) -> WasmtimeResult<i32> {
    println!("in wasm_32::c");
    imports::c(
        fenv,
        mod_id_ofs as usize,
        name_ofs as usize,
        name_len,
        arg_len,
        gas_limit,
    )
}

pub(crate) fn emit(
    fenv: Caller<Env>,
    topic_ofs: u32,
    topic_len: u32,
    arg_len: u32,
) -> WasmtimeResult<()> {
    imports::emit(fenv, topic_ofs as usize, topic_len, arg_len)
}

pub(crate) fn owner(fenv: Caller<Env>, mod_id_ofs: u32) -> WasmtimeResult<i32> {
    imports::owner(fenv, mod_id_ofs as usize)
}
