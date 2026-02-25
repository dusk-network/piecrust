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
    ARGBUF_LEN, CONTRACT_ID_BYTES, ContractError, ContractId,
};

use crate::Error;
use crate::config::BYTE_STORE_COST;
use crate::instance::{Env, WrappedInstance};
use crate::session::INIT_METHOD;

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
                    return Err(Error::InvalidFunction(
                        import_name.to_string(),
                    ));
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

    if end > mem_len {
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

    let name_len = name_len as usize;

    let (name, arg, gas_remaining) = {
        let instance = env.self_instance();
        check_ptr(instance, name_ofs, name_len)?;
        check_arg(instance, arg_len)?;

        let name = instance.with_memory(|buf| {
            // performance: use a dedicated buffer here?
            core::str::from_utf8(&buf[name_ofs..][..name_len])
                .map(ToOwned::to_owned)
        })?;

        let arg = instance.with_arg_buf(|arg_buf| {
            let arg_len = arg_len as usize;
            Vec::from(&arg_buf[..arg_len])
        });

        let gas_remaining = instance.get_remaining_gas();

        (name, arg, gas_remaining)
    };

    let host_query = env
        .host_query_arc(&name)
        .ok_or_else(move || Error::MissingHostQuery(name))?;
    let mut query_arg: Box<dyn Any> = Box::new(());
    let query_cost = host_query.deserialize_and_price(&arg, &mut query_arg);

    if gas_remaining < query_cost {
        env.self_instance().set_remaining_gas(0);
        Err(Error::OutOfGas)?;
    }

    let instance = env.self_instance();
    instance.set_remaining_gas(gas_remaining - query_cost);

    Ok(instance
        .with_arg_buf_mut(|arg_buf| host_query.execute(&query_arg, arg_buf)))
}

pub(crate) fn hd(
    mut fenv: Caller<Env>,
    name_ofs: usize,
    name_len: u32,
) -> WasmtimeResult<u32> {
    let env = fenv.data_mut();

    let name_len = name_len as usize;

    let name = {
        let instance = env.self_instance();
        check_ptr(instance, name_ofs, name_len)?;

        instance.with_memory(|buf| {
            // performance: use a dedicated buffer here?
            core::str::from_utf8(&buf[name_ofs..][..name_len])
                .map(ToOwned::to_owned)
        })?
    };

    let data = env.meta(&name).unwrap_or_default();

    let instance = env.self_instance();
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
    let name_len = name_len as usize;

    let write_contract_error = |env: &mut Env, err: Error| {
        let c_err = ContractError::from(err);
        env.self_instance().with_arg_buf_mut(|buf| {
            c_err.to_parts(buf);
        });
        c_err.into()
    };

    let parsed = {
        let instance = env.self_instance();

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

        let (callee_id, name, arg) =
            instance.with_memory_mut(|memory| -> Result<_, Error> {
                let mut callee_bytes = [0; CONTRACT_ID_BYTES];
                callee_bytes.copy_from_slice(
                    &memory[callee_ofs..callee_ofs + CONTRACT_ID_BYTES],
                );
                let callee_id = ContractId::from_bytes(callee_bytes);

                let name =
                    core::str::from_utf8(&memory[name_ofs..][..name_len])?;

                let arg = Vec::from(&memory[argbuf_ofs..][..arg_len as usize]);
                Ok((callee_id, name.to_owned(), arg))
            })?;

        Ok::<_, Error>((caller_remaining, callee_limit, callee_id, name, arg))
    };

    let (caller_remaining, callee_limit, callee_id, name, arg) = match parsed {
        Ok(parsed) => parsed,
        Err(err) => return Ok(write_contract_error(env, err)),
    };

    let callee_stack_element = match env.push_callstack(callee_id, callee_limit)
    {
        Ok(stack_element) => stack_element,
        Err(err) => return Ok(write_contract_error(env, err)),
    };

    let callee_result = (|| -> Result<(i32, Vec<u8>, u64), Error> {
        let callee = env
            .instance(&callee_stack_element.contract_id)
            .expect("callee instance should exist");

        callee.snap().map_err(|err| Error::MemorySnapshotFailure {
            reason: None,
            io: Arc::new(err),
        })?;

        if name == INIT_METHOD {
            return Err(Error::InitalizationError(
                "init call not allowed".into(),
            ));
        }

        callee.write_argument(&arg);
        let ret_len = callee
            .call(&name, arg.len() as u32, callee_limit)
            .map_err(Error::normalize)?;
        check_arg(callee, ret_len as u32)?;

        let mut ret_data = vec![0u8; ret_len as usize];
        callee.read_argument(&mut ret_data);
        let callee_spent = callee_limit - callee.get_remaining_gas();

        Ok((ret_len, ret_data, callee_spent))
    })();

    match callee_result {
        Ok((ret_len, ret_data, callee_spent)) => {
            env.move_up_call_tree(callee_spent);
            let caller = env.self_instance();
            caller.with_arg_buf_mut(|buf| {
                buf[..ret_len as usize].copy_from_slice(&ret_data);
            });
            caller.set_remaining_gas(caller_remaining - callee_spent);
            Ok(ret_len)
        }
        Err(mut err) => {
            if let Err(io_err) = env.revert_callstack() {
                err = Error::MemorySnapshotFailure {
                    reason: Some(Arc::new(err)),
                    io: Arc::new(io_err),
                };
            }
            env.move_up_prune_call_tree();
            env.self_instance()
                .set_remaining_gas(caller_remaining - callee_limit);
            Ok(write_contract_error(env, err))
        }
    }
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

fn caller(mut env: Caller<Env>) -> i32 {
    let env = env.data_mut();

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

fn callstack(mut env: Caller<Env>) -> i32 {
    let env = env.data_mut();
    let call_ids: Vec<_> =
        env.call_ids().into_iter().skip(1).copied().collect();
    let caller_count = call_ids.len();
    let instance = env.self_instance();

    for (i, contract_id) in call_ids.into_iter().enumerate() {
        instance.with_arg_buf_mut(|buf| {
            buf[i * CONTRACT_ID_BYTES..(i + 1) * CONTRACT_ID_BYTES]
                .copy_from_slice(contract_id.as_bytes());
        });
    }
    caller_count as i32
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
    let msg = {
        let instance = env.self_instance();
        check_arg(instance, msg_len)?;

        instance.with_arg_buf(|buf| {
            let slice = &buf[..msg_len as usize];
            let msg = std::str::from_utf8(slice).map_err(Error::Utf8)?;
            Ok::<_, Error>(msg.to_owned())
        })?
    };

    env.register_debug(&msg);
    println!("CONTRACT DEBUG {msg}");

    Ok(())
}

fn limit(fenv: Caller<Env>) -> u64 {
    fenv.data().limit()
}

fn spent(mut fenv: Caller<Env>) -> u64 {
    let env = fenv.data_mut();
    let limit = env.limit();
    let remaining = env.self_instance().get_remaining_gas();

    limit - remaining
}

fn panic(mut fenv: Caller<Env>, arg_len: u32) -> WasmtimeResult<()> {
    let env = fenv.data_mut();
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
    let env = fenv.data_mut();

    {
        let instance = env.self_instance();
        check_ptr(instance, mod_id_ofs, CONTRACT_ID_BYTES)?;
    }

    match get_metadata(env, mod_id_ofs).map(|metadata| metadata.owner.clone()) {
        None => Ok(0),
        Some(owner) => {
            let instance = env.self_instance();

            instance.with_arg_buf_mut(|arg| {
                arg[..owner.len()].copy_from_slice(&owner)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::MAX_CALL_DEPTH;
    use crate::{ContractData, SessionData, VM, contract_bytecode};

    const OWNER: [u8; 32] = [0u8; 32];
    const LIMIT: u64 = 1_000_000;

    #[test]
    fn check_ptr_boundary() {
        let vm = VM::ephemeral().expect("ephemeral VM should be created");
        let mut session = vm
            .session(SessionData::builder())
            .expect("session should be created");
        let contract_id = session
            .deploy(
                contract_bytecode!("counter"),
                ContractData::builder().owner(OWNER),
                LIMIT,
            )
            .expect("contract should deploy");

        session
            .push_callstack(contract_id, LIMIT)
            .expect("callstack push should instantiate contract");
        let instance = session
            .instance(&contract_id)
            .expect("instance should exist after push_callstack");

        // Accessing the full memory should be valid
        let mem_len = instance.with_memory(|mem| mem.len());
        let check_res = check_ptr(instance, 0, mem_len);
        println!("{check_res:?}");
        assert!(check_res.is_ok());

        // Accessing 1 over the bound should be invalid
        let check_res = check_ptr(instance, 0, mem_len + 1).unwrap_err();
        assert!(matches!(
            check_res,
            Error::MemoryAccessOutOfBounds {
                offset: 0,
                len,
                mem_len
            } if len == mem_len + 1
        ));
    }

    #[test]
    fn push_callstack_enforces_max_depth() {
        let vm = VM::ephemeral().expect("ephemeral VM should be created");
        let mut session = vm
            .session(SessionData::builder())
            .expect("session should be created");
        let contract_id = session
            .deploy(
                contract_bytecode!("counter"),
                ContractData::builder().owner(OWNER),
                LIMIT,
            )
            .expect("contract should deploy");

        for _ in 0..MAX_CALL_DEPTH {
            session
                .push_callstack(contract_id, LIMIT)
                .expect("push within depth limit should succeed");
        }

        let err = session
            .push_callstack(contract_id, LIMIT)
            .expect_err("depth overflow should fail");
        assert!(matches!(err, Error::SessionError(_)));
    }
}
