// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::sync::Arc;

use dusk_wasmtime::{Caller, Result as WasmtimeResult};
use piecrust_uplink::{
    ContractError, ContractId, ARGBUF_LEN, CONTRACT_ID_BYTES,
};

use crate::imports::{check_arg, check_ptr, POINT_PASS_PCT};
use crate::instance::Env;
use crate::Error;

pub(crate) fn hq(
    mut fenv: Caller<Env>,
    name_ofs: u64,
    name_len: u32,
    arg_len: u32,
) -> WasmtimeResult<u32> {
    let env = fenv.data_mut();

    let instance = env.self_instance();

    let name_ofs = name_ofs as usize;
    let name_len = name_len as usize;

    check_ptr(instance, name_ofs, name_len)?;
    check_arg(instance, arg_len)?;

    let name = instance.with_memory(|buf| {
        // performance: use a dedicated buffer here?
        core::str::from_utf8(&buf[name_ofs..][..name_len])
            .map(ToOwned::to_owned)
    })?;

    Ok(instance
        .with_arg_buf_mut(|buf| env.host_query(&name, buf, arg_len))
        .ok_or(Error::MissingHostQuery(name))?)
}

pub(crate) fn hd(
    mut fenv: Caller<Env>,
    name_ofs: u64,
    name_len: u32,
) -> WasmtimeResult<u32> {
    let env = fenv.data_mut();

    let instance = env.self_instance();

    let name_ofs = name_ofs as usize;
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
    mod_id_ofs: u64,
    name_ofs: u64,
    name_len: u32,
    arg_len: u32,
    points_limit: u64,
) -> WasmtimeResult<i32> {
    let env = fenv.data_mut();

    let instance = env.self_instance();

    let mod_id_ofs = mod_id_ofs as usize;
    let name_ofs = name_ofs as usize;
    let name_len = name_len as usize;

    check_ptr(instance, mod_id_ofs, CONTRACT_ID_BYTES)?;
    check_ptr(instance, name_ofs, name_len)?;
    check_arg(instance, arg_len)?;

    let argbuf_ofs = instance.arg_buffer_offset();

    let caller_remaining = instance.get_remaining_points();

    let callee_limit = if points_limit > 0 && points_limit < caller_remaining {
        points_limit
    } else {
        caller_remaining * POINT_PASS_PCT / 100
    };

    let with_memory = |memory: &mut [u8]| -> Result<_, Error> {
        let arg_buf = &memory[argbuf_ofs..][..ARGBUF_LEN];

        let mut mod_id = ContractId::uninitialized();
        mod_id.as_bytes_mut().copy_from_slice(
            &memory[mod_id_ofs..][..std::mem::size_of::<ContractId>()],
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

        let name = core::str::from_utf8(&memory[name_ofs..][..name_len])?;

        let arg = &arg_buf[..arg_len as usize];

        callee.write_argument(arg);
        let ret_len = callee
            .call(name, arg.len() as u32, callee_limit)
            .map_err(Error::normalize)?;
        check_arg(callee, ret_len as u32)?;

        // copy back result
        callee.read_argument(&mut memory[argbuf_ofs..][..ret_len as usize]);

        let callee_remaining = callee.get_remaining_points();
        let callee_spent = callee_limit - callee_remaining;

        Ok((ret_len, callee_spent))
    };

    let ret = match instance.with_memory_mut(with_memory) {
        Ok((ret_len, callee_spent)) => {
            env.move_up_call_tree(callee_spent);
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

pub(crate) fn emit(
    mut fenv: Caller<Env>,
    topic_ofs: u64,
    topic_len: u32,
    arg_len: u32,
) -> WasmtimeResult<()> {
    let env = fenv.data_mut();
    let instance = env.self_instance();

    let topic_ofs = topic_ofs as usize;
    let topic_len = topic_len as usize;

    check_ptr(instance, topic_ofs, topic_len)?;
    check_arg(instance, arg_len)?;

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
