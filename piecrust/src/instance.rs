// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::io;
use std::ops::{Deref, DerefMut};

use dusk_wasmtime::{Instance, Module, Mutability, Store, ValType};
use piecrust_uplink::{ContractId, Event, ARGBUF_LEN};

use crate::contract::WrappedContract;
use crate::imports::Imports;
use crate::session::Session;
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
    pub fn self_instance<'b>(&self) -> &'b mut WrappedInstance {
        let stack_element = self
            .session
            .nth_from_top(0)
            .expect("there should be at least one element in the call stack");
        self.instance(&stack_element.contract_id)
            .expect("instance should exist")
    }

    pub fn instance<'b>(
        &self,
        contract_id: &ContractId,
    ) -> Option<&'b mut WrappedInstance> {
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
                if param != ValType::I32 {
                    return Err(Error::InvalidFunction(func_name.to_string()));
                }

                // There must be only one result with type `I32`.
                let mut results = func_ty.results();
                if results.len() != 1 {
                    return Err(Error::InvalidFunction(func_name.to_string()));
                }
                let result = results.next().unwrap();
                if result != ValType::I32 {
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

        let wrapped = WrappedInstance {
            store,
            instance,
            arg_buf_ofs,
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
        self.with_arg_buf_mut(|buf| buf[..arg.len()].copy_from_slice(arg))
    }

    // Read argument from instance
    pub(crate) fn read_argument(&mut self, arg: &mut [u8]) {
        self.with_arg_buf(|buf| arg.copy_from_slice(&buf[..arg.len()]))
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
        f(&self.memory)
    }

    pub(crate) fn with_memory_mut<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        f(&mut self.memory)
    }

    /// Returns the current length of the memory.
    pub(crate) fn mem_len(&self) -> usize {
        self.memory.current_len
    }

    /// Sets the length of the memory.
    pub(crate) fn set_len(&mut self, len: usize) {
        self.memory.current_len = len;
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

    pub(crate) fn write_bytes_to_arg_buffer(&mut self, buf: &[u8]) -> u32 {
        self.with_arg_buf_mut(|arg_buffer| {
            arg_buffer[..buf.len()].copy_from_slice(buf);
            buf.len() as u32
        })
    }

    pub fn call(
        &mut self,
        method_name: &str,
        arg_len: u32,
        limit: u64,
    ) -> Result<i32, Error> {
        let fun = self
            .instance
            .get_typed_func::<u32, i32>(&mut self.store, method_name)?;

        self.set_remaining_points(limit);

        fun.call(&mut self.store, arg_len)
            .map_err(|e| map_call_err(self, e))
    }

    pub fn set_remaining_points(&mut self, limit: u64) {
        let remaining = self.store.fuel_remaining().expect("Fuel is enabled");
        self.store
            .consume_fuel(remaining)
            .expect("Consuming all fuel should succeed");
        self.store
            .add_fuel(limit)
            .expect("Adding fuel should succeed");
    }

    pub fn get_remaining_points(&mut self) -> u64 {
        self.store.fuel_remaining().expect("Fuel should be enabled")
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
}

fn map_call_err(
    instance: &mut WrappedInstance,
    err: dusk_wasmtime::Error,
) -> Error {
    if instance.get_remaining_points() == 0 {
        return Error::OutOfPoints;
    }

    err.into()
}
