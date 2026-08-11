// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::io;
use std::ops::{Deref, DerefMut, Range};

use dusk_wasmtime::{Instance, Module, Mutability, Store, ValType};
use piecrust_uplink::{ARGBUF_LEN, ContractId, Event, HOST_CALL_FRAME_MAX_LEN};

use crate::Error;
use crate::config::HOST_CALL_COPY_BYTE_GAS;
use crate::imports::Imports;
use crate::session::Session;
use crate::store::Memory;

pub struct WrappedInstance {
    instance: Instance,
    arg_buf_ofs: usize,
    argument_abi: ArgumentAbi,
    host_call_frames: Vec<HostCallFrame>,
    store: Store<Env>,
    memory: Memory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArgumentAbi {
    Legacy,
    HostBacked,
}

#[derive(Debug)]
struct HostCallFrame {
    input: Vec<u8>,
    output: Vec<u8>,
    result: Vec<u8>,
    result_len: usize,
}

fn checked_range(
    offset: usize,
    len: usize,
    total: usize,
) -> Result<Range<usize>, Error> {
    let end =
        offset
            .checked_add(len)
            .ok_or(Error::MemoryAccessOutOfBounds {
                offset,
                len,
                mem_len: total,
            })?;
    if end > total {
        return Err(Error::MemoryAccessOutOfBounds {
            offset,
            len,
            mem_len: total,
        });
    }
    Ok(offset..end)
}

pub(crate) struct Env {
    self_id: ContractId,
    session: Session,
}

impl Deref for Env {
    type Target = Session;

    fn deref(&self) -> &Self::Target {
        &self.session
    }
}

impl DerefMut for Env {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.session
    }
}

impl Env {
    pub fn self_instance(&mut self) -> &mut WrappedInstance {
        let stack_element = self
            .session
            .nth_from_top(0)
            .expect("there should be at least one element in the call stack");
        self.instance(&stack_element.contract_id)
            .expect("instance should exist")
    }

    pub fn instance(
        &mut self,
        contract_id: &ContractId,
    ) -> Option<&mut WrappedInstance> {
        self.session.instance(contract_id)
    }

    pub fn limit(&self) -> u64 {
        self.session
            .nth_from_top(0)
            .expect("there should be at least one element in the call stack")
            .limit
    }

    pub fn emit(&mut self, topic: String, data: Vec<u8>) {
        let event = Event {
            source: self.self_id,
            topic,
            data,
            reverted: false,
        };

        self.session.push_event(event);
    }

    pub fn self_contract_id(&self) -> &ContractId {
        &self.self_id
    }
}

impl WrappedInstance {
    pub fn new(
        session: Session,
        contract_id: ContractId,
        contract: &Module,
        memory: Memory,
    ) -> Result<Self, Error> {
        let mut memory = memory;
        let engine = session.engine().clone();
        let host_backed_abi_enabled = session.host_backed_abi_enabled();

        let env = Env {
            self_id: contract_id,
            session,
        };

        let module = contract.clone();
        let mut store = Store::new(&engine, env);

        // Ensure there is at most one memory exported, and that it is called
        // "memory".
        let n_memories = module
            .exports()
            .filter(|exp| exp.ty().memory().is_some())
            .count();

        if n_memories != 1 {
            return Err(Error::TooManyMemories(n_memories));
        }

        let is_64 = module
            .exports()
            .filter_map(|exp| exp.ty().memory().map(|mem_ty| mem_ty.is_64()))
            .next()
            .unwrap();

        // Ensure that every exported function has a signature that matches the
        // calling convention `F: I32 -> I32`.
        for exp in module.exports() {
            let exp_ty = exp.ty();
            if let Some(func_ty) = exp_ty.func() {
                let func_name = exp.name();

                // There must be only one parameter with type `I32`.
                let mut params = func_ty.params();
                if params.len() != 1 {
                    return Err(Error::InvalidFunction(func_name.to_string()));
                }
                let param = params.next().unwrap();
                if !param.matches(&ValType::I32) {
                    return Err(Error::InvalidFunction(func_name.to_string()));
                }

                // There must be only one result with type `I32`.
                let mut results = func_ty.results();
                if results.len() != 1 {
                    return Err(Error::InvalidFunction(func_name.to_string()));
                }
                let result = results.next().unwrap();
                if !result.matches(&ValType::I32) {
                    return Err(Error::InvalidFunction(func_name.to_string()));
                }
            }
        }

        let has_legacy_abi = module.exports().any(|export| {
            export.name() == "A" && export.ty().global().is_some()
        });
        let has_host_backed_abi = module.exports().any(|export| {
            export.name() == "B" && export.ty().global().is_some()
        });
        let argument_abi = match (has_legacy_abi, has_host_backed_abi) {
            (true, _) => ArgumentAbi::Legacy,
            (false, true) if host_backed_abi_enabled => ArgumentAbi::HostBacked,
            (false, true) => return Err(Error::HostBackedAbiNotEnabled),
            _ => return Err(Error::InvalidArgumentBuffer),
        };

        let imports = Imports::for_module(
            &mut store,
            &module,
            is_64,
            argument_abi == ArgumentAbi::HostBacked,
        )?;
        let instance = Instance::new(&mut store, &module, &imports)?;

        let arg_buf_ofs = match argument_abi {
            ArgumentAbi::Legacy => {
                // Ensure there is a global exported named `A`, whose value is
                // in the memory.
                match instance.get_global(&mut store, "A") {
                    Some(global) => {
                        let ty = global.ty(&mut store);

                        if ty.mutability() != Mutability::Const {
                            return Err(Error::InvalidArgumentBuffer);
                        }

                        let val = global.get(&mut store);

                        if is_64 {
                            val.i64().ok_or(Error::InvalidArgumentBuffer)?
                                as usize
                        } else {
                            val.i32().ok_or(Error::InvalidArgumentBuffer)?
                                as usize
                        }
                    }
                    _ => return Err(Error::InvalidArgumentBuffer),
                }
            }
            ArgumentAbi::HostBacked => {
                let global = instance
                    .get_global(&mut store, "B")
                    .ok_or(Error::InvalidArgumentBuffer)?;
                if global.ty(&mut store).mutability() != Mutability::Const {
                    return Err(Error::InvalidArgumentBuffer);
                }
                let value = global.get(&mut store);
                if is_64 {
                    value.i64().ok_or(Error::InvalidArgumentBuffer)? as usize
                } else {
                    value.i32().ok_or(Error::InvalidArgumentBuffer)? as usize
                }
            }
        };

        let invalid_arg_buf = match argument_abi {
            ArgumentAbi::Legacy => arg_buf_ofs + ARGBUF_LEN >= memory.len(),
            ArgumentAbi::HostBacked => arg_buf_ofs
                .checked_add(ARGBUF_LEN)
                .is_none_or(|end| end > memory.current_len()),
        };
        if invalid_arg_buf {
            return Err(Error::InvalidArgumentBuffer);
        }

        // A memory is no longer new after one instantiation
        memory.set_is_new(false);

        let wrapped = WrappedInstance {
            store,
            instance,
            arg_buf_ofs,
            argument_abi,
            host_call_frames: Vec::new(),
            memory,
        };

        Ok(wrapped)
    }

    pub(crate) fn snap(&mut self) -> io::Result<()> {
        self.memory.snap()?;
        Ok(())
    }

    pub(crate) fn revert(&mut self) -> io::Result<()> {
        self.memory.revert()?;
        Ok(())
    }

    pub(crate) fn apply(&mut self) -> io::Result<()> {
        self.memory.apply()?;
        Ok(())
    }

    // Write argument into instance
    pub(crate) fn write_argument(&mut self, arg: &[u8]) {
        self.with_arg_buf_mut(|buf| {
            // Using `ptr::copy` instead of `[T].copy_from_slice` because it's
            // possible for `arg` and `buf` to point to the same
            // location, in the case of an inter-contract
            // call to the same contract and `[T].copy_from_slice` requires that
            // the two slices must be non-overlapping.
            unsafe {
                core::ptr::copy(arg.as_ptr(), buf.as_mut_ptr(), arg.len());
            }
        })
    }

    // Read argument from instance
    pub(crate) fn read_argument(&mut self, arg: &mut [u8]) {
        self.with_arg_buf(|buf| {
            // Using `ptr::copy` for the same reason as in `write_argument`.
            unsafe {
                core::ptr::copy(buf.as_ptr(), arg.as_mut_ptr(), arg.len());
            }
        })
    }

    pub(crate) fn read_bytes_from_arg_buffer(&self, arg_len: u32) -> Vec<u8> {
        self.with_arg_buf(|abuf| {
            let slice = &abuf[..arg_len as usize];
            slice.to_vec()
        })
    }

    pub(crate) fn with_memory<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        self.memory.with_bytes(f)
    }

    pub(crate) fn with_memory_mut<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        self.memory.with_bytes_mut(f)
    }

    /// Returns the current length of the memory.
    pub(crate) fn mem_len(&self) -> usize {
        self.memory.current_len()
    }

    /// Sets the length of the memory.
    pub(crate) fn set_len(&mut self, len: usize) {
        self.memory.set_current_len(len);
    }

    #[cfg(test)]
    pub(crate) fn fail_next_snap(&mut self) {
        self.memory.fail_next_snap();
    }

    #[cfg(test)]
    pub(crate) fn fail_next_revert(&mut self) {
        self.memory.fail_next_revert();
    }

    #[cfg(test)]
    pub(crate) fn fail_next_apply(&mut self) {
        self.memory.fail_next_apply();
    }

    pub(crate) fn with_arg_buf<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        let offset = self.arg_buf_ofs;
        self.with_memory(
            |memory_bytes| f(&memory_bytes[offset..][..ARGBUF_LEN]),
        )
    }

    pub(crate) fn with_arg_buf_mut<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let offset = self.arg_buf_ofs;
        self.with_memory_mut(|memory_bytes| {
            f(&mut memory_bytes[offset..][..ARGBUF_LEN])
        })
    }

    pub(crate) fn write_bytes_to_arg_buffer(
        &mut self,
        buf: &[u8],
    ) -> Result<u32, Error> {
        self.with_arg_buf_mut(|arg_buffer| {
            if buf.len() > arg_buffer.len() {
                return Err(Error::MemoryAccessOutOfBounds {
                    offset: 0,
                    len: buf.len(),
                    mem_len: ARGBUF_LEN,
                });
            }

            arg_buffer[..buf.len()].copy_from_slice(buf);
            // It is safe to cast to u32 because the length of the buffer is
            // guaranteed to be less than 4GiB.
            Ok(buf.len() as u32)
        })
    }

    pub fn call(
        &mut self,
        method_name: &str,
        arg_len: u32,
        limit: u64,
    ) -> Result<i32, Error> {
        if !self.is_function_exported(method_name) {
            return Err(Error::InvalidFunction(method_name.to_owned()));
        }
        let fun = self
            .instance
            .get_typed_func::<u32, i32>(&mut self.store, method_name)?;

        self.set_remaining_gas(limit);

        fun.call(&mut self.store, arg_len)
            .map_err(|e| map_call_err(self, e))
    }

    pub fn set_remaining_gas(&mut self, limit: u64) {
        self.store.set_fuel(limit).expect("Fuel is enabled");
    }

    pub fn get_remaining_gas(&mut self) -> u64 {
        self.store.get_fuel().expect("Fuel is enabled")
    }

    pub fn is_function_exported<N: AsRef<str>>(&mut self, name: N) -> bool {
        self.instance
            .get_func(&mut self.store, name.as_ref())
            .is_some()
    }

    #[allow(unused)]
    pub fn print_state(&self) {
        self.with_memory(|mem| {
            const CSZ: usize = 128;
            const RSZ: usize = 16;

            for (chunk_nr, chunk) in mem.chunks(CSZ).enumerate() {
                if chunk[..] != [0; CSZ][..] {
                    for (row_nr, row) in chunk.chunks(RSZ).enumerate() {
                        let ofs = chunk_nr * CSZ + row_nr * RSZ;

                        print!("{ofs:08x}:");

                        for (i, byte) in row.iter().enumerate() {
                            if i % 4 == 0 {
                                print!(" ");
                            }

                            let buf_start = self.arg_buf_ofs;
                            let buf_end = buf_start + ARGBUF_LEN;

                            if ofs + i >= buf_start && ofs + i < buf_end {
                                print!("{byte:02x}");
                                print!(" ");
                            } else {
                                print!("{byte:02x} ")
                            }
                        }

                        println!();
                    }
                }
            }
        });
    }

    pub fn arg_buffer_offset(&self) -> usize {
        self.arg_buf_ofs
    }

    pub(crate) const fn argument_capacity(&self) -> usize {
        match self.argument_abi {
            ArgumentAbi::Legacy => ARGBUF_LEN,
            ArgumentAbi::HostBacked => HOST_CALL_FRAME_MAX_LEN,
        }
    }

    pub(crate) fn is_host_backed_abi(&self) -> bool {
        self.argument_abi == ArgumentAbi::HostBacked
    }

    pub(crate) fn call_host_backed(
        &mut self,
        method_name: &str,
        input: Vec<u8>,
        limit: u64,
    ) -> Result<Vec<u8>, Error> {
        if !self.is_host_backed_abi() {
            return Err(Error::InvalidArgumentBuffer);
        }
        if input.len() > HOST_CALL_FRAME_MAX_LEN {
            return Err(Error::ArgumentBufferOverflow {
                len: input.len(),
                max_len: HOST_CALL_FRAME_MAX_LEN,
            });
        }

        let input_len = input.len() as u32;
        self.host_call_frames.push(HostCallFrame {
            input,
            output: Vec::new(),
            result: Vec::new(),
            result_len: 0,
        });
        let result = self.call(method_name, input_len, limit);
        let frame = self
            .host_call_frames
            .pop()
            .expect("host call frame was just pushed");

        let returned_len = usize::try_from(result?)
            .map_err(|_| Error::InvalidArgumentBuffer)?;
        if returned_len != frame.output.len() {
            return Err(Error::SessionError(
                "B return length does not match host output".into(),
            ));
        }
        Ok(frame.output)
    }

    pub(crate) fn host_call_input_len(&self) -> Result<u32, Error> {
        let frame = self.host_call_frames.last().ok_or_else(|| {
            Error::SessionError("B call frame is not active".into())
        })?;
        Ok(frame.input.len() as u32)
    }

    pub(crate) fn copy_host_call_input(
        &mut self,
        input_offset: usize,
        guest_offset: usize,
        len: usize,
    ) -> Result<(), Error> {
        let input_len = self
            .host_call_frames
            .last()
            .ok_or_else(|| {
                Error::SessionError("B call frame is not active".into())
            })?
            .input
            .len();
        let input_range = checked_range(input_offset, len, input_len)?;
        let guest_range = checked_range(guest_offset, len, self.mem_len())?;

        self.charge_host_call_copy(len)?;
        let input = &self
            .host_call_frames
            .last()
            .expect("host call frame was checked")
            .input[input_range];
        self.memory.with_bytes_mut(|memory| {
            memory[guest_range].copy_from_slice(input);
        });
        Ok(())
    }

    pub(crate) fn set_host_call_output(
        &mut self,
        guest_offset: usize,
        len: usize,
    ) -> Result<(), Error> {
        if len > HOST_CALL_FRAME_MAX_LEN {
            return Err(Error::ArgumentBufferOverflow {
                len,
                max_len: HOST_CALL_FRAME_MAX_LEN,
            });
        }
        if self.host_call_frames.is_empty() {
            return Err(Error::SessionError(
                "B call frame is not active".into(),
            ));
        }

        let guest_range = checked_range(guest_offset, len, self.mem_len())?;

        self.charge_host_call_copy(len)?;
        let output = self
            .memory
            .with_bytes(|memory| memory[guest_range].to_vec());
        self.host_call_frames
            .last_mut()
            .expect("host call frame was checked")
            .output = output;
        Ok(())
    }

    pub(crate) fn clear_host_result(&mut self) -> Result<(), Error> {
        self.host_call_frames
            .last_mut()
            .ok_or_else(|| {
                Error::SessionError("B call frame is not active".into())
            })?
            .result_len = 0;
        Ok(())
    }

    pub(crate) fn execute_host_query(
        &mut self,
        arg: &[u8],
        execute: impl FnOnce(&mut [u8]) -> u32,
    ) -> Result<u32, Error> {
        let frame = self.host_call_frames.last_mut().ok_or_else(|| {
            Error::SessionError("B call frame is not active".into())
        })?;
        if frame.result.len() < HOST_CALL_FRAME_MAX_LEN {
            frame.result.resize(HOST_CALL_FRAME_MAX_LEN, 0);
        }
        frame.result[..arg.len()].copy_from_slice(arg);

        let ret_len = execute(&mut frame.result) as usize;
        if ret_len > frame.result.len() {
            return Err(Error::ArgumentBufferOverflow {
                len: ret_len,
                max_len: frame.result.len(),
            });
        }
        frame.result_len = ret_len;
        Ok(ret_len as u32)
    }

    pub(crate) fn set_host_result(
        &mut self,
        result: Vec<u8>,
    ) -> Result<(), Error> {
        if result.len() > HOST_CALL_FRAME_MAX_LEN {
            return Err(Error::ArgumentBufferOverflow {
                len: result.len(),
                max_len: HOST_CALL_FRAME_MAX_LEN,
            });
        }
        let frame = self.host_call_frames.last_mut().ok_or_else(|| {
            Error::SessionError("B call frame is not active".into())
        })?;
        frame.result_len = result.len();
        frame.result = result;
        Ok(())
    }

    pub(crate) fn host_result_len(&self) -> Result<u32, Error> {
        let frame = self.host_call_frames.last().ok_or_else(|| {
            Error::SessionError("B call frame is not active".into())
        })?;
        Ok(frame.result_len as u32)
    }

    pub(crate) fn copy_host_result(
        &mut self,
        result_offset: usize,
        guest_offset: usize,
        len: usize,
    ) -> Result<(), Error> {
        let result_len = self
            .host_call_frames
            .last()
            .ok_or_else(|| {
                Error::SessionError("B call frame is not active".into())
            })?
            .result_len;
        let result_range = checked_range(result_offset, len, result_len)?;
        let guest_range = checked_range(guest_offset, len, self.mem_len())?;

        self.charge_host_call_copy(len)?;
        let result = &self
            .host_call_frames
            .last()
            .expect("host call frame was checked")
            .result[result_range];
        self.memory.with_bytes_mut(|memory| {
            memory[guest_range].copy_from_slice(result);
        });
        Ok(())
    }

    pub(crate) fn copy_guest_to_host(
        &mut self,
        guest_offset: usize,
        len: usize,
    ) -> Result<Vec<u8>, Error> {
        if len > HOST_CALL_FRAME_MAX_LEN {
            return Err(Error::ArgumentBufferOverflow {
                len,
                max_len: HOST_CALL_FRAME_MAX_LEN,
            });
        }
        if self.host_call_frames.is_empty() {
            return Err(Error::SessionError(
                "B call frame is not active".into(),
            ));
        }

        let guest_range = checked_range(guest_offset, len, self.mem_len())?;

        self.charge_host_call_copy(len)?;
        Ok(self
            .memory
            .with_bytes(|memory| memory[guest_range].to_vec()))
    }

    fn charge_host_call_copy(&mut self, len: usize) -> Result<(), Error> {
        let cost = u64::try_from(len)
            .unwrap_or(u64::MAX)
            .saturating_mul(HOST_CALL_COPY_BYTE_GAS);
        let remaining = self.get_remaining_gas();
        if cost > remaining {
            self.set_remaining_gas(0);
            return Err(Error::OutOfGas);
        }
        self.set_remaining_gas(remaining - cost);
        Ok(())
    }
}

fn map_call_err(
    instance: &mut WrappedInstance,
    err: dusk_wasmtime::Error,
) -> Error {
    if instance.get_remaining_gas() == 0 {
        return Error::OutOfGas;
    }

    err.into()
}
