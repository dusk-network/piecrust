// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::io;
use std::ops::{Deref, DerefMut};

use crate::contract::contract_instance::{ContractInstance, InstanceUtil};
use dusk_wasmtime::{Instance, Module, Mutability, Store, ValType};
use piecrust_uplink::{ContractId, Event, ARGBUF_LEN};

use crate::contract::WrappedContract;
use crate::imports::Imports;
use crate::session::Session;
use crate::session_env::SessionEnv;
use crate::store::Memory;
use crate::Error;

pub struct WrappedInstance {
    instance: Instance,
    arg_buf_ofs: usize,
    store: Store<Env>,
    memory: Memory,
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
    pub fn self_instance<'b>(&mut self) -> &mut dyn ContractInstance {
        let stack_element = self
            .session
            .nth_from_top(0)
            .expect("there should be at least one element in the call stack");
        self.instance(&stack_element.contract_id)
            .expect("instance should exist")
    }

    pub fn instance<'b>(
        &mut self,
        contract_id: &ContractId,
    ) -> Option<&mut dyn ContractInstance> {
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
        contract: &WrappedContract,
        memory: Memory,
    ) -> Result<Self, Error> {
        let mut memory = memory;
        let engine = session.engine().clone();

        let env = Env {
            self_id: contract_id,
            session,
        };

        let module =
            unsafe { Module::deserialize(&engine, contract.as_bytes())? };
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

        let imports = Imports::for_module(&mut store, &module, is_64)?;
        let instance = Instance::new(&mut store, &module, &imports)?;

        // Ensure there is a global exported named `A`, whose value is in the
        // memory.
        let arg_buf_ofs = match instance.get_global(&mut store, "A") {
            Some(global) => {
                let ty = global.ty(&mut store);

                if ty.mutability() != Mutability::Const {
                    return Err(Error::InvalidArgumentBuffer);
                }

                let val = global.get(&mut store);

                if is_64 {
                    val.i64().ok_or(Error::InvalidArgumentBuffer)? as usize
                } else {
                    val.i32().ok_or(Error::InvalidArgumentBuffer)? as usize
                }
            }
            _ => return Err(Error::InvalidArgumentBuffer),
        };

        if arg_buf_ofs + ARGBUF_LEN >= memory.len() {
            return Err(Error::InvalidArgumentBuffer);
        }

        // A memory is no longer new after one instantiation
        memory.is_new = false;

        let wrapped = WrappedInstance {
            store,
            instance,
            arg_buf_ofs,
            memory,
        };

        Ok(wrapped)
    }
}

impl ContractInstance for WrappedInstance {
    fn snap(&mut self) -> io::Result<()> {
        self.memory.snap()?;
        Ok(())
    }

    fn revert(&mut self) -> io::Result<()> {
        self.memory.revert()?;
        Ok(())
    }

    fn apply(&mut self) -> io::Result<()> {
        self.memory.apply()?;
        Ok(())
    }

    // Write argument into instance
    fn write_argument(&mut self, arg: &[u8]) {
        let buf_ofs = self.get_arg_buf_ofs();
        InstanceUtil::with_arg_buf_mut(self.get_memory_mut(), buf_ofs, |buf| {
            // Using `ptr::copy` instead of `[T].copy_from_slice` because
            // it's possible for `arg` and `buf` to point to
            // the same location, in the case of an
            // inter-contract call to the same contract and
            // `[T].copy_from_slice` requires that
            // the two slices must be non-overlapping.
            unsafe {
                core::ptr::copy(arg.as_ptr(), buf.as_mut_ptr(), arg.len());
            }
        })
    }

    // Read argument from instance
    fn read_argument(&mut self, arg: &mut [u8]) {
        InstanceUtil::with_arg_buf(
            self.get_memory(),
            self.get_arg_buf_ofs(),
            |buf| {
                // Using `ptr::copy` for the same reason as in `write_argument`.
                unsafe {
                    core::ptr::copy(buf.as_ptr(), arg.as_mut_ptr(), arg.len());
                }
            },
        )
    }

    fn read_bytes_from_arg_buffer(&self, arg_len: u32) -> Vec<u8> {
        InstanceUtil::with_arg_buf(
            self.get_memory(),
            self.get_arg_buf_ofs(),
            |abuf| {
                let slice = &abuf[..arg_len as usize];
                slice.to_vec()
            },
        )
    }

    // fn with_memory<F, R>(&self, f: F) -> R
    // where
    //     F: FnOnce(&[u8]) -> R,
    // {
    //     f(&self.memory)
    // }

    // fn with_memory_mut<F, R>(&mut self, f: F) -> R
    // where
    //     F: FnOnce(&mut [u8]) -> R,
    // {
    //     f(&mut self.memory)
    // }

    /// Returns the current length of the memory.
    fn mem_len(&self) -> usize {
        self.memory.current_len
    }

    /// Sets the length of the memory.
    fn set_len(&mut self, len: usize) {
        self.memory.current_len = len;
    }

    // fn with_arg_buf<F, R>(&self, f: F) -> R
    // where
    //     F: FnOnce(&[u8]) -> R,
    // {
    //     let offset = self.arg_buf_ofs;
    //     self.with_memory(|memory_bytes| {
    //         f(&memory_bytes[offset..][..ARGBUF_LEN])
    //     })
    // }

    // fn with_arg_buf_mut<F, R>(&mut self, f: F) -> R
    // where
    //     F: FnOnce(&mut [u8]) -> R,
    // {
    //     let offset = self.arg_buf_ofs;
    //     self.with_memory_mut(|memory_bytes| {
    //         f(&mut memory_bytes[offset..][..ARGBUF_LEN])
    //     })
    // }

    fn write_bytes_to_arg_buffer(&mut self, buf: &[u8]) -> Result<u32, Error> {
        let buf_ofs = self.get_arg_buf_ofs();
        InstanceUtil::with_arg_buf_mut(
            self.get_memory_mut(),
            buf_ofs,
            |arg_buffer| {
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
            },
        )
    }

    fn set_remaining_gas(&mut self, limit: u64) {
        self.store.set_fuel(limit).expect("Fuel is enabled");
    }

    fn get_remaining_gas(&mut self) -> u64 {
        self.store.get_fuel().expect("Fuel is enabled")
    }

    fn is_function_exported(&mut self, name: &str) -> bool {
        self.instance.get_func(&mut self.store, name).is_some()
    }

    #[allow(unused)]
    fn print_state(&self) {
        InstanceUtil::with_memory(self.get_memory(), |mem| {
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

    fn arg_buffer_offset(&self) -> usize {
        self.arg_buf_ofs
    }

    fn map_call_err(&mut self, err: dusk_wasmtime::Error) -> Error {
        if self.get_remaining_gas() == 0 {
            return Error::OutOfGas;
        }

        err.into()
    }

    fn call(
        &mut self,
        method_name: &str,
        arg_len: u32,
        limit: u64,
    ) -> Result<i32, Error> {
        let fun = self
            .instance
            .get_typed_func::<u32, i32>(&mut self.store, method_name)?;

        // self.set_remaining_gas(limit);
        self.store.set_fuel(limit).expect("Fuel is enabled");

        fun.call(&mut self.store, arg_len)
            .map_err(|e| self.map_call_err(e))
    }

    fn get_memory(&self) -> &[u8] {
        &***self.memory
    }

    fn get_memory_mut(&mut self) -> &mut [u8] {
        &mut ***self.memory
    }

    fn get_arg_buf_ofs(&self) -> usize {
        self.arg_buf_ofs
    }
}
