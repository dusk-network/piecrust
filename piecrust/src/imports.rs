// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::sync::Arc;

use piecrust_uplink::{
    ContractError, ContractId, ARGBUF_LEN, CONTRACT_ID_BYTES,
};
use wasmer::{imports, Function, FunctionEnv, FunctionEnvMut};

use crate::instance::{Env, WrappedInstance};
use crate::Error;

const POINT_PASS_PCT: u64 = 93;

pub(crate) struct DefaultImports;

impl DefaultImports {
    pub fn default(store: &mut wasmer::Store, env: Env) -> wasmer::Imports {
        let fenv = FunctionEnv::new(store, env);

        #[allow(unused_mut)]
        let mut imports = imports! {
            "env" => {
                "caller" => Function::new_typed_with_env(store, &fenv, caller),
                "c" => Function::new_typed_with_env(store, &fenv, c),
                "hq" => Function::new_typed_with_env(store, &fenv, hq),
                "hd" => Function::new_typed_with_env(store, &fenv, hd),
                "emit" => Function::new_typed_with_env(store, &fenv, emit),
                "feed" => Function::new_typed_with_env(store, &fenv, feed),
                "limit" => Function::new_typed_with_env(store, &fenv, limit),
                "spent" => Function::new_typed_with_env(store, &fenv, spent),
                "panic" => Function::new_typed_with_env(store, &fenv, panic),
                "owner" => Function::new_typed_with_env(store, &fenv, owner),
                "self_id" => Function::new_typed_with_env(store, &fenv, self_id),
            }
        };

        #[cfg(feature = "debug")]
        imports.define(
            "env",
            "hdebug",
            Function::new_typed_with_env(store, &fenv, hdebug),
        );

        imports
    }
}

fn check_ptr(
    instance: &WrappedInstance,
    offset: u32,
    len: u32,
) -> Result<(), Error> {
    let mem_len = instance.with_memory(|mem| mem.len());

    let offset = offset as usize;
    let len = len as usize;

    if offset + len >= mem_len {
        return Err(Error::MemoryAccessOutOfBounds {
            offset,
            len,
            mem_len,
        });
    }

    Ok(())
}

fn check_arg(instance: &WrappedInstance, arg_len: u32) -> Result<(), Error> {
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

fn caller(env: FunctionEnvMut<Env>) {
    let env = env.data();

    let mod_id = env
        .nth_from_top(1)
        .map_or(ContractId::uninitialized(), |elem| elem.contract_id);

    env.self_instance().with_arg_buffer(|arg| {
        arg[..std::mem::size_of::<ContractId>()]
            .copy_from_slice(mod_id.as_bytes())
    })
}

fn c(
    mut fenv: FunctionEnvMut<Env>,
    mod_id_ofs: u32,
    name_ofs: u32,
    name_len: u32,
    arg_len: u32,
    points_limit: u64,
) -> Result<i32, Error> {
    let env = fenv.data_mut();

    let instance = env.self_instance();

    check_ptr(instance, mod_id_ofs, CONTRACT_ID_BYTES as u32)?;
    check_ptr(instance, name_ofs, name_len)?;
    check_arg(instance, arg_len)?;

    let argbuf_ofs = instance.arg_buffer_offset();

    let caller_remaining = instance
        .get_remaining_points()
        .expect("there should be points remaining");

    let callee_limit = if points_limit > 0 && points_limit < caller_remaining {
        points_limit
    } else {
        caller_remaining * POINT_PASS_PCT / 100
    };

    let with_memory = |memory: &mut [u8]| -> Result<_, Error> {
        let arg_buf = &memory[argbuf_ofs..][..ARGBUF_LEN];

        let mut mod_id = ContractId::uninitialized();
        mod_id.as_bytes_mut().copy_from_slice(
            &memory[mod_id_ofs as usize..][..std::mem::size_of::<ContractId>()],
        );

        let callee_stack_element = env
            .push_callstack(mod_id, callee_limit)
            .expect("pushing to the callstack should succeed");
        let callee = env
            .instance(&callee_stack_element.contract_id)
            .expect("callee instance should exist");

        callee.snap().map_err(|err| Error::MemorySnapshotFailure {
            reason: None,
            io: Arc::new(err),
        })?;

        let name = core::str::from_utf8(
            &memory[name_ofs as usize..][..name_len as usize],
        )?;

        let arg = &arg_buf[..arg_len as usize];

        callee.write_argument(arg);
        let ret_len = callee
            .call(name, arg.len() as u32, callee_limit)
            .map_err(Error::normalize)?;
        check_arg(callee, ret_len as u32)?;

        // copy back result
        callee.read_argument(&mut memory[argbuf_ofs..][..ret_len as usize]);

        let callee_remaining = callee
            .get_remaining_points()
            .expect("there should be points remaining");
        let callee_spent = callee_limit - callee_remaining;

        Ok((ret_len, callee_spent))
    };

    let ret = match instance.with_memory_mut(with_memory) {
        Ok((ret_len, callee_spent)) => {
            env.move_up_call_tree();
            instance.set_remaining_points(caller_remaining - callee_spent);
            ret_len
        }
        Err(mut err) => {
            if let Err(io_err) = env.revert_callstack() {
                err = Error::MemorySnapshotFailure {
                    reason: Some(Arc::new(err)),
                    io: Arc::new(io_err),
                };
            }
            env.move_up_prune_call_tree();
            instance.set_remaining_points(caller_remaining - callee_limit);

            ContractError::from(err).into()
        }
    };

    Ok(ret)
}

fn hq(
    mut fenv: FunctionEnvMut<Env>,
    name_ofs: u32,
    name_len: u32,
    arg_len: u32,
) -> Result<u32, Error> {
    let env = fenv.data_mut();

    let instance = env.self_instance();

    check_ptr(instance, name_ofs, arg_len)?;
    check_arg(instance, arg_len)?;

    let name_ofs = name_ofs as usize;
    let name_len = name_len as usize;

    let name = instance.with_memory(|buf| {
        // performance: use a dedicated buffer here?
        core::str::from_utf8(&buf[name_ofs..][..name_len])
            .map(ToOwned::to_owned)
    })?;

    instance
        .with_arg_buffer(|buf| env.host_query(&name, buf, arg_len))
        .ok_or(Error::MissingHostQuery(name))
}

fn hd(
    mut fenv: FunctionEnvMut<Env>,
    name_ofs: u32,
    name_len: u32,
) -> Result<u32, Error> {
    let env = fenv.data_mut();

    let instance = env.self_instance();

    check_ptr(instance, name_ofs, name_len)?;

    let name_ofs = name_ofs as usize;
    let name_len = name_len as usize;

    let name = instance.with_memory(|buf| {
        // performance: use a dedicated buffer here?
        core::str::from_utf8(&buf[name_ofs..][..name_len])
            .map(ToOwned::to_owned)
    })?;

    let data = env.meta(&name).unwrap_or_default();

    instance.with_arg_buffer(|buf| {
        buf[..data.len()].copy_from_slice(&data);
    });

    Ok(data.len() as u32)
}

fn emit(
    mut fenv: FunctionEnvMut<Env>,
    topic_ofs: u32,
    topic_len: u32,
    arg_len: u32,
) -> Result<(), Error> {
    let env = fenv.data_mut();
    let instance = env.self_instance();

    check_ptr(instance, topic_ofs, topic_len)?;
    check_arg(instance, arg_len)?;

    let data = instance.with_arg_buffer(|buf| {
        let arg_len = arg_len as usize;
        Vec::from(&buf[..arg_len])
    });

    let topic_ofs = topic_ofs as usize;
    let topic_len = topic_len as usize;

    let topic = instance.with_memory(|buf| {
        // performance: use a dedicated buffer here?
        core::str::from_utf8(&buf[topic_ofs..][..topic_len])
            .map(ToOwned::to_owned)
    })?;

    env.emit(topic, data);

    Ok(())
}

fn feed(mut fenv: FunctionEnvMut<Env>, arg_len: u32) -> Result<(), Error> {
    let env = fenv.data_mut();
    let instance = env.self_instance();

    check_arg(instance, arg_len)?;

    let data = instance.with_arg_buffer(|buf| {
        let arg_len = arg_len as usize;
        Vec::from(&buf[..arg_len])
    });

    env.push_feed(data)
}

#[cfg(feature = "debug")]
fn hdebug(mut fenv: FunctionEnvMut<Env>, msg_len: u32) -> Result<(), Error> {
    let env = fenv.data_mut();
    let instance = env.self_instance();

    check_arg(instance, msg_len)?;

    instance.with_arg_buffer(|buf| {
        let slice = &buf[..msg_len as usize];

        let msg = match std::str::from_utf8(slice) {
            Ok(msg) => msg,
            Err(err) => return Err(Error::Utf8(err)),
        };

        env.register_debug(msg);
        println!("CONTRACT DEBUG {msg}");

        Ok(())
    })
}

fn limit(fenv: FunctionEnvMut<Env>) -> u64 {
    fenv.data().limit()
}

fn spent(fenv: FunctionEnvMut<Env>) -> u64 {
    let env = fenv.data();
    let instance = env.self_instance();

    let limit = env.limit();
    let remaining = instance
        .get_remaining_points()
        .expect("there should be remaining points");

    limit - remaining
}

fn panic(fenv: FunctionEnvMut<Env>, arg_len: u32) -> Result<(), Error> {
    let env = fenv.data();
    let instance = env.self_instance();

    check_arg(instance, arg_len)?;

    instance.with_arg_buffer(|buf| {
        let slice = &buf[..arg_len as usize];

        let msg = match std::str::from_utf8(slice) {
            Ok(msg) => msg,
            Err(err) => return Err(Error::Utf8(err)),
        };

        Err(Error::ContractPanic(msg.to_owned()))
    })
}

fn owner(fenv: FunctionEnvMut<Env>) -> u32 {
    let env = fenv.data();
    let self_id = env.self_contract_id();
    let contract_metadata = env
        .contract_metadata(self_id)
        .expect("contract metadata should exist");
    let slice = contract_metadata.owner.as_slice();
    let len = slice.len();
    env.self_instance()
        .with_arg_buffer(|arg| arg[..len].copy_from_slice(slice));
    len as u32
}

fn self_id(fenv: FunctionEnvMut<Env>) -> u32 {
    let env = fenv.data();
    let self_id = env.self_contract_id();
    let contract_metadata = env
        .contract_metadata(self_id)
        .expect("contract metadata should exist");
    let slice = contract_metadata.contract_id.as_bytes();
    let len = slice.len();
    env.self_instance()
        .with_arg_buffer(|arg| arg[..len].copy_from_slice(slice));
    len as u32
}
