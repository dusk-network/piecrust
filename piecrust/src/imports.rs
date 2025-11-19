// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

mod wasm32;
mod wasm64;

use std::any::Any;
use std::sync::Arc;

use crate::contract::ContractMetadata;
use dusk_wasmtime::{
    Caller, Extern, Func, Module, Result as WasmtimeResult, Store,
};
use piecrust_uplink::{
    ContractError, ContractId, ARGBUF_LEN, CONTRACT_ID_BYTES,
};
use tracing::debug;

use crate::config::BYTE_STORE_COST;
use crate::instance::{Env, WrappedInstance};
use crate::session::INIT_METHOD;
use crate::Error;

pub const GAS_PASS_PCT: u64 = 93;

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
            "callstack" => Func::wrap(store, callstack),
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
            "owner" => match is_64 {
                false => Func::wrap(store, wasm32::owner),
                true => Func::wrap(store, wasm64::owner),
            },
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

    let end =
        offset
            .checked_add(len)
            .ok_or(Error::MemoryAccessOutOfBounds {
                offset,
                len,
                mem_len,
            })?;

    if end >= mem_len {
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

pub(crate) fn hq(
    mut fenv: Caller<Env>,
    name_ofs: usize,
    name_len: u32,
    arg_len: u32,
) -> WasmtimeResult<u32> {
    let env = fenv.data_mut();

    let instance = env.self_instance();

    let name_len = name_len as usize;

    check_ptr(instance, name_ofs, name_len)?;
    check_arg(instance, arg_len)?;

    let name = instance.with_memory(|buf| {
        // performance: use a dedicated buffer here?
        core::str::from_utf8(&buf[name_ofs..][..name_len])
            .map(ToOwned::to_owned)
    })?;

    // Get the host query if it exists.
    let host_query =
        env.host_query(&name).ok_or(Error::MissingHostQuery(name))?;
    let mut arg: Box<dyn Any> = Box::new(());

    // Price the query, allowing for an early exit if the gas is insufficient.
    let query_cost = instance.with_arg_buf(|arg_buf| {
        let arg_len = arg_len as usize;
        let arg_buf = &arg_buf[..arg_len];
        host_query.deserialize_and_price(arg_buf, &mut arg)
    });

    // If the gas is insufficient, return an error.
    let gas_remaining = instance.get_remaining_gas();
    if gas_remaining < query_cost {
        instance.set_remaining_gas(0);
        Err(Error::OutOfGas)?;
    }
    instance.set_remaining_gas(gas_remaining - query_cost);

    // Execute the query and return the result.
    Ok(instance.with_arg_buf_mut(|arg_buf| host_query.execute(&arg, arg_buf)))
}

pub(crate) fn hd(
    mut fenv: Caller<Env>,
    name_ofs: usize,
    name_len: u32,
) -> WasmtimeResult<u32> {
    let env = fenv.data_mut();

    let instance = env.self_instance();

    let name_len = name_len as usize;

    check_ptr(instance, name_ofs, name_len)?;

    let name = instance.with_memory(|buf| {
        // performance: use a dedicated buffer here?
        core::str::from_utf8(&buf[name_ofs..][..name_len])
            .map(ToOwned::to_owned)
    })?;

    let data = env.meta(&name).unwrap_or_default();

    instance.with_arg_buf_mut(|buf| {
        buf[..data.len()].copy_from_slice(&data);
    });

    Ok(data.len() as u32)
}

pub(crate) fn c(
    mut fenv: Caller<Env>,
    callee_ofs: usize,
    name_ofs: usize,
    name_len: u32,
    arg_len: u32,
    gas_limit: u64,
) -> WasmtimeResult<i32> {
    let env = fenv.data_mut();

    let instance = env.self_instance();

    let name_len = name_len as usize;

    check_ptr(instance, callee_ofs, CONTRACT_ID_BYTES)?;
    check_ptr(instance, name_ofs, name_len)?;
    check_arg(instance, arg_len)?;

    let argbuf_ofs = instance.arg_buffer_offset();

    let caller_remaining = instance.get_remaining_gas();

    let callee_limit = if gas_limit > 0 && gas_limit < caller_remaining {
        gas_limit
    } else {
        let div = caller_remaining / 100 * GAS_PASS_PCT;
        let rem = caller_remaining % 100 * GAS_PASS_PCT / 100;
        div + rem
    };

    enum WithMemoryError {
        BeforePush(Error),
        AfterPush(Error),
    }

    let with_memory = |memory: &mut [u8]| -> Result<_, WithMemoryError> {
        let arg_buf = &memory[argbuf_ofs..][..ARGBUF_LEN];

        let mut callee_bytes = [0; CONTRACT_ID_BYTES];
        callee_bytes.copy_from_slice(
            &memory[callee_ofs..callee_ofs + CONTRACT_ID_BYTES],
        );
        let callee_id = ContractId::from_bytes(callee_bytes);

        let callee_stack_element = env
            .push_callstack(callee_id, callee_limit)
            .map_err(WithMemoryError::BeforePush)?;
        let callee = env
            .instance(&callee_stack_element.contract_id)
            .expect("callee instance should exist");

        debug!("fn c: snapshotting callee memory");
        callee
            .instance_snap()
            .map_err(|err| Error::MemorySnapshotFailure {
                reason: None,
                io: Arc::new(err),
            })
            .map_err(WithMemoryError::AfterPush)?;

        let name = core::str::from_utf8(&memory[name_ofs..][..name_len])
            .map_err(|e| WithMemoryError::AfterPush(e.into()))?;
        if name == INIT_METHOD {
            return Err(WithMemoryError::AfterPush(Error::InitalizationError(
                "init call not allowed".into(),
            )));
        }

        let arg = &arg_buf[..arg_len as usize];

        callee.write_argument(arg);
        let ret_len = callee
            .call(name, arg.len() as u32, callee_limit)
            .map_err(Error::normalize)
            .map_err(WithMemoryError::AfterPush)?;
        check_arg(callee, ret_len as u32)
            .map_err(WithMemoryError::AfterPush)?;

        // copy back result
        callee.read_argument(&mut memory[argbuf_ofs..][..ret_len as usize]);

        let callee_remaining = callee.get_remaining_gas();
        let callee_spent = callee_limit - callee_remaining;

        Ok((ret_len, callee_spent))
    };

    let ret = match instance.with_memory_mut(with_memory) {
        Ok((ret_len, callee_spent)) => {
            env.move_up_call_tree(callee_spent);
            instance.set_remaining_gas(caller_remaining - callee_spent);
            ret_len
        }
        Err(WithMemoryError::BeforePush(err)) => {
            let c_err = ContractError::from(err);
            instance.with_arg_buf_mut(|buf| {
                c_err.to_parts(buf);
            });
            c_err.into()
        }
        Err(WithMemoryError::AfterPush(mut err)) => {
            debug!("fn c: invoke revert_callstack due to {:?}", err);
            if let Err(io_err) = env.revert_callstack() {
                err = Error::MemorySnapshotFailure {
                    reason: Some(Arc::new(err)),
                    io: Arc::new(io_err),
                };
            }
            debug!("fn c: call tree: {:?}", env.call_tree().call_ids());

            env.move_up_prune_call_tree();
            //env.clear_stack_and_instances(); // TODO: isn't
            // clear_stack_and_instances needed here too?
            // NOTE: for some reason when adding this, we also get a SIGSEGV
            // when doing only activate_tx
            instance.set_remaining_gas(caller_remaining - callee_limit);

            let c_err = ContractError::from(err);
            instance.with_arg_buf_mut(|buf| {
                c_err.to_parts(buf);
            });
            c_err.into()
        }
    };

    Ok(ret)
}

pub(crate) fn emit(
    mut fenv: Caller<Env>,
    topic_ofs: usize,
    topic_len: u32,
    arg_len: u32,
) -> WasmtimeResult<()> {
    let env = fenv.data_mut();
    let instance = env.self_instance();

    let topic_len = topic_len as usize;

    check_ptr(instance, topic_ofs, topic_len)?;
    check_arg(instance, arg_len)?;

    // charge for each byte emitted in an event
    let gas_remaining = instance.get_remaining_gas();
    let gas_cost = BYTE_STORE_COST as u64 * (topic_len as u64 + arg_len as u64);

    if gas_cost > gas_remaining {
        instance.set_remaining_gas(0);
        Err(Error::OutOfGas)?;
    }
    instance.set_remaining_gas(gas_remaining - gas_cost);

    let data = instance.with_arg_buf(|buf| {
        let arg_len = arg_len as usize;
        Vec::from(&buf[..arg_len])
    });

    let topic = instance.with_memory(|buf| {
        // performance: use a dedicated buffer here?
        core::str::from_utf8(&buf[topic_ofs..][..topic_len])
            .map(ToOwned::to_owned)
    })?;

    env.emit(topic, data);

    Ok(())
}

fn caller(env: Caller<Env>) -> i32 {
    let env = env.data();

    match env.nth_from_top(1) {
        Some(call_tree_elem) => {
            let instance = env.self_instance();
            instance.with_arg_buf_mut(|buf| {
                let caller = call_tree_elem.contract_id;
                buf[..CONTRACT_ID_BYTES].copy_from_slice(caller.as_bytes());
            });
            1
        }
        None => 0,
    }
}

fn callstack(env: Caller<Env>) -> i32 {
    let env = env.data();
    let instance = env.self_instance();

    let mut i = 0usize;
    for contract_id in env.call_ids().iter().skip(1) {
        instance.with_arg_buf_mut(|buf| {
            buf[i * CONTRACT_ID_BYTES..(i + 1) * CONTRACT_ID_BYTES]
                .copy_from_slice(contract_id.as_bytes());
        });
        i += 1;
    }
    i as i32
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
    let remaining = instance.get_remaining_gas();

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

        Err(Error::Panic(msg.to_owned()))
    })?)
}

fn get_metadata(
    env: &mut Env,
    contract_id_ofs: usize,
) -> Option<&ContractMetadata> {
    // The null pointer is always zero, so we can use this to check if the
    // caller wants their own ID.
    if contract_id_ofs == 0 {
        let self_id = env.self_contract_id().to_owned();

        let contract_metadata = env
            .contract_metadata(&self_id)
            .expect("contract metadata should exist");

        Some(contract_metadata)
    } else {
        let instance = env.self_instance();

        let contract_id = instance.with_memory(|memory| {
            let mut contract_id_bytes = [0u8; CONTRACT_ID_BYTES];
            contract_id_bytes.copy_from_slice(
                &memory[contract_id_ofs..][..CONTRACT_ID_BYTES],
            );
            ContractId::from_bytes(contract_id_bytes)
        });

        env.contract_metadata(&contract_id)
    }
}

fn owner(mut fenv: Caller<Env>, mod_id_ofs: usize) -> WasmtimeResult<i32> {
    let instance = fenv.data().self_instance();
    check_ptr(instance, mod_id_ofs, CONTRACT_ID_BYTES)?;
    let env = fenv.data_mut();
    match get_metadata(env, mod_id_ofs) {
        None => Ok(0),
        Some(metadata) => {
            let owner = metadata.owner.as_slice();

            instance.with_arg_buf_mut(|arg| {
                arg[..owner.len()].copy_from_slice(owner)
            });

            Ok(1)
        }
    }
}

fn self_id(mut fenv: Caller<Env>) {
    let env = fenv.data_mut();
    let self_id = env.self_contract_id().to_owned();
    let contract_metadata = env
        .contract_metadata(&self_id)
        .expect("contract metadata should exist");
    let slice = contract_metadata.contract_id.to_bytes();
    let len = slice.len();
    env.self_instance()
        .with_arg_buf_mut(|arg| arg[..len].copy_from_slice(&slice));
}
