// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

mod wasm32;
mod wasm64;

use dusk_wasmtime::{
    Caller, Extern, Func, Module, Result as WasmtimeResult, Store,
};
use piecrust_uplink::{ContractId, ARGBUF_LEN};

use crate::instance::{Env, WrappedInstance};
use crate::Error;

pub const POINT_PASS_PCT: u64 = 93;

pub(crate) struct Imports;

impl Imports {
    /// Makes a vector of imports for the given module.
    pub fn for_module(
        store: &mut Store<Env>,
        module: &Module,
        is_64: bool,
    ) -> Result<Vec<Extern>, Error> {
        let max_imports = 12;
        let mut imports = Vec::with_capacity(max_imports);

        for import in module.imports() {
            let import_name = import.name();

            match Self::import(store, import_name, is_64) {
                None => {
                    return Err(Error::InvalidFunction(import_name.to_string()))
                }
                Some(func) => {
                    imports.push(func.into());
                }
            }
        }

        Ok(imports)
    }

    fn import(store: &mut Store<Env>, name: &str, is_64: bool) -> Option<Func> {
        Some(match name {
            "caller" => Func::wrap(store, caller),
            "c" => match is_64 {
                false => Func::wrap(store, wasm32::c),
                true => Func::wrap(store, wasm64::c),
            },
            "hq" => match is_64 {
                false => Func::wrap(store, wasm32::hq),
                true => Func::wrap(store, wasm64::hq),
            },
            "hd" => match is_64 {
                false => Func::wrap(store, wasm32::hd),
                true => Func::wrap(store, wasm64::hd),
            },
            "emit" => match is_64 {
                false => Func::wrap(store, wasm32::emit),
                true => Func::wrap(store, wasm64::emit),
            },
            "feed" => Func::wrap(store, feed),
            "limit" => Func::wrap(store, limit),
            "spent" => Func::wrap(store, spent),
            "panic" => Func::wrap(store, panic),
            "owner" => Func::wrap(store, owner),
            "self_id" => Func::wrap(store, self_id),
            #[cfg(feature = "debug")]
            "hdebug" => Func::wrap(store, hdebug),
            _ => return None,
        })
    }
}

pub fn check_ptr(
    instance: &WrappedInstance,
    offset: usize,
    len: usize,
) -> Result<(), Error> {
    let mem_len = instance.with_memory(|mem| mem.len());

    if offset + len >= mem_len {
        return Err(Error::MemoryAccessOutOfBounds {
            offset,
            len,
            mem_len,
        });
    }

    Ok(())
}

pub fn check_arg(
    instance: &WrappedInstance,
    arg_len: u32,
) -> Result<(), Error> {
    let mem_len = instance.with_memory(|mem| mem.len());

    let arg_ofs = instance.arg_buffer_offset();
    let arg_len = arg_len as usize;

    if arg_len > ARGBUF_LEN {
        return Err(Error::MemoryAccessOutOfBounds {
            offset: arg_ofs,
            len: arg_len,
            mem_len,
        });
    }

    if arg_ofs + arg_len > mem_len {
        return Err(Error::MemoryAccessOutOfBounds {
            offset: arg_ofs,
            len: arg_len,
            mem_len,
        });
    }

    Ok(())
}

fn caller(env: Caller<Env>) {
    let env = env.data();

    let mod_id = env
        .nth_from_top(1)
        .map_or(ContractId::uninitialized(), |elem| elem.contract_id);

    env.self_instance().with_arg_buf_mut(|arg| {
        arg[..std::mem::size_of::<ContractId>()]
            .copy_from_slice(mod_id.as_bytes())
    })
}

fn feed(mut fenv: Caller<Env>, arg_len: u32) -> WasmtimeResult<()> {
    let env = fenv.data_mut();
    let instance = env.self_instance();

    check_arg(instance, arg_len)?;

    let data = instance.with_arg_buf(|buf| {
        let arg_len = arg_len as usize;
        Vec::from(&buf[..arg_len])
    });

    Ok(env.push_feed(data)?)
}

#[cfg(feature = "debug")]
fn hdebug(mut fenv: Caller<Env>, msg_len: u32) -> WasmtimeResult<()> {
    let env = fenv.data_mut();
    let instance = env.self_instance();

    check_arg(instance, msg_len)?;

    Ok(instance.with_arg_buf(|buf| {
        let slice = &buf[..msg_len as usize];

        let msg = match std::str::from_utf8(slice) {
            Ok(msg) => msg,
            Err(err) => return Err(Error::Utf8(err)),
        };

        env.register_debug(msg);
        println!("CONTRACT DEBUG {msg}");

        Ok(())
    })?)
}

fn limit(fenv: Caller<Env>) -> u64 {
    fenv.data().limit()
}

fn spent(fenv: Caller<Env>) -> u64 {
    let env = fenv.data();
    let instance = env.self_instance();

    let limit = env.limit();
    let remaining = instance.get_remaining_points();

    limit - remaining
}

fn panic(fenv: Caller<Env>, arg_len: u32) -> WasmtimeResult<()> {
    let env = fenv.data();
    let instance = env.self_instance();

    check_arg(instance, arg_len)?;

    Ok(instance.with_arg_buf(|buf| {
        let slice = &buf[..arg_len as usize];

        let msg = match std::str::from_utf8(slice) {
            Ok(msg) => msg,
            Err(err) => return Err(Error::Utf8(err)),
        };

        Err(Error::ContractPanic(msg.to_owned()))
    })?)
}

fn owner(fenv: Caller<Env>) {
    let env = fenv.data();
    let self_id = env.self_contract_id();
    let contract_metadata = env
        .contract_metadata(self_id)
        .expect("contract metadata should exist");
    let slice = contract_metadata.owner.as_slice();
    let len = slice.len();
    env.self_instance()
        .with_arg_buf_mut(|arg| arg[..len].copy_from_slice(slice));
}

fn self_id(fenv: Caller<Env>) {
    let env = fenv.data();
    let self_id = env.self_contract_id();
    let contract_metadata = env
        .contract_metadata(self_id)
        .expect("contract metadata should exist");
    let slice = contract_metadata.contract_id.as_bytes();
    let len = slice.len();
    env.self_instance()
        .with_arg_buf_mut(|arg| arg[..len].copy_from_slice(slice));
}
