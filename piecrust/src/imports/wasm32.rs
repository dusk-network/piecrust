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
    imports::hq(fenv, name_ofs as usize, name_len, arg_len)
}

pub(crate) fn hd(
    fenv: Caller<Env>,
    name_ofs: u32,
    name_len: u32,
) -> WasmtimeResult<u32> {
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

#[allow(clippy::too_many_arguments)]
pub(crate) fn deploy(
    fenv: Caller<Env>,
    bytecode_ofs: u32,
    bytecode_len: u64,
    init_arg_ofs: u32,
    init_arg_len: u32,
    owner_ofs: u32,
    owner_len: u32,
    deploy_nonce: u64,
    gas_limit: u64,
) -> WasmtimeResult<i32> {
    imports::deploy(
        fenv,
        bytecode_ofs as usize,
        bytecode_len,
        init_arg_ofs as usize,
        init_arg_len,
        owner_ofs as usize,
        owner_len,
        deploy_nonce,
        gas_limit,
    )
}
