// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;
use std::sync::Arc;
use std::{io, slice};

use colored::*;
use wasmer::wasmparser::Operator;
use wasmer::{CompilerConfig, RuntimeError, Tunables, TypedFunction};
use wasmer_compiler_singlepass::Singlepass;
use wasmer_middlewares::metering::{
    get_remaining_points, set_remaining_points, MeteringPoints,
};
use wasmer_middlewares::Metering;
use wasmer_types::{
    MemoryError, MemoryStyle, MemoryType, TableStyle, TableType,
};
use wasmer_vm::{LinearMemory, VMMemory, VMTable, VMTableDefinition};

use piecrust_uplink::{ContractId, Event, ARGBUF_LEN};

use crate::contract::WrappedContract;
use crate::imports::DefaultImports;
use crate::session::Session;
use crate::store::{Memory, MAX_MEM_SIZE};
use crate::Error;

pub struct WrappedInstance {
    instance: wasmer::Instance,
    arg_buf_ofs: usize,
    store: wasmer::Store,
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

/// Convenience methods for dealing with our custom store
pub struct Store;

impl Store {
    const INITIAL_POINT_LIMIT: u64 = 10_000_000;

    pub fn new_store() -> wasmer::Store {
        Self::with_creator(|compiler_config| {
            wasmer::Store::new(compiler_config)
        })
    }

    pub fn new_store_with_tunables(
        tunables: impl Tunables + Send + Sync + 'static,
    ) -> wasmer::Store {
        Self::with_creator(|compiler_config| {
            wasmer::Store::new_with_tunables(compiler_config, tunables)
        })
    }

    fn with_creator<F>(f: F) -> wasmer::Store
    where
        F: FnOnce(Singlepass) -> wasmer::Store,
    {
        let metering =
            Arc::new(Metering::new(Self::INITIAL_POINT_LIMIT, cost_function));

        let mut compiler_config = Singlepass::default();
        compiler_config.push_middleware(metering);

        f(compiler_config)
    }
}

impl WrappedInstance {
    pub fn new(
        session: Session,
        contract_id: ContractId,
        contract: &WrappedContract,
        memory: Memory,
    ) -> Result<Self, Error> {
        let tunables = InstanceTunables::new(memory.clone());
        let mut store = Store::new_store_with_tunables(tunables);

        let env = Env {
            self_id: contract_id,
            session,
        };

        let imports = DefaultImports::default(&mut store, env);

        let module = unsafe {
            wasmer::Module::deserialize(&store, contract.as_bytes())?
        };

        let instance = wasmer::Instance::new(&mut store, &module, &imports)?;

        // Ensure there is a global exported named `A`, whose value is in the
        // memory.
        let arg_buf_ofs =
            match instance.exports.get_global("A")?.get(&mut store) {
                wasmer::Value::I32(i) => i as usize,
                _ => return Err(Error::InvalidArgumentBuffer),
            };
        if arg_buf_ofs + ARGBUF_LEN >= MAX_MEM_SIZE {
            return Err(Error::InvalidArgumentBuffer);
        }

        // Ensure there is at most one memory exported, and that it is called
        // "memory".
        let n_memories = instance.exports.iter().memories().count();
        if n_memories > 1 {
            return Err(Error::TooManyMemories(n_memories));
        }
        let _ = instance.exports.get_memory("memory")?;

        // Ensure that every exported function has a signature that matches the
        // calling convention `F: I32 -> I32`.
        for (fname, func) in instance.exports.iter().functions() {
            let func_ty = func.ty(&store);
            if func_ty.params() != [wasmer::Type::I32]
                || func_ty.results() != [wasmer::Type::I32]
            {
                return Err(Error::InvalidFunctionSignature(fname.clone()));
            }
        }

        let wrapped = WrappedInstance {
            store,
            instance,
            arg_buf_ofs,
            memory,
        };

        Ok(wrapped)
    }

    pub(crate) fn snap(&self) -> io::Result<()> {
        let mut memory = self.memory.write();
        memory.snap()?;
        Ok(())
    }

    pub(crate) fn revert(&self) -> io::Result<()> {
        let mut memory = self.memory.write();
        memory.revert()?;
        Ok(())
    }

    pub(crate) fn apply(&self) -> io::Result<()> {
        let mut memory = self.memory.write();
        memory.apply()?;
        Ok(())
    }

    // Write argument into instance
    pub(crate) fn write_argument(&mut self, arg: &[u8]) {
        self.with_arg_buffer(|buf| buf[..arg.len()].copy_from_slice(arg))
    }

    // Read argument from instance
    pub(crate) fn read_argument(&mut self, arg: &mut [u8]) {
        self.with_arg_buffer(|buf| arg.copy_from_slice(&buf[..arg.len()]))
    }

    pub(crate) fn read_bytes_from_arg_buffer(&self, arg_len: u32) -> Vec<u8> {
        self.with_arg_buffer(|abuf| {
            let slice = &abuf[..arg_len as usize];
            slice.to_vec()
        })
    }

    pub(crate) fn with_memory<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        let mem = self.instance.exports.get_memory("memory").expect(
            "memory export should be checked at contract creation time",
        );
        let view = mem.view(&self.store);
        let memory_bytes = unsafe {
            let slice = view.data_unchecked();
            let ptr = slice.as_ptr();
            slice::from_raw_parts(ptr, MAX_MEM_SIZE)
        };
        f(memory_bytes)
    }

    pub(crate) fn with_memory_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mem = self.instance.exports.get_memory("memory").expect(
            "memory export should be checked at contract creation time",
        );
        let view = mem.view(&self.store);
        let memory_bytes = unsafe {
            let slice = view.data_unchecked_mut();
            let ptr = slice.as_mut_ptr();
            slice::from_raw_parts_mut(ptr, MAX_MEM_SIZE)
        };
        f(memory_bytes)
    }

    /// Returns the current length of the memory.
    pub(crate) fn mem_len(&self) -> usize {
        self.memory.read().inner.def.current_length
    }

    /// Sets the length of the memory.
    pub(crate) fn set_len(&mut self, len: usize) {
        self.memory.write().inner.def.current_length = len;
    }

    pub(crate) fn with_arg_buffer<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        self.with_memory_mut(|memory_bytes| {
            let offset = self.arg_buf_ofs;
            f(&mut memory_bytes[offset..][..ARGBUF_LEN])
        })
    }

    pub(crate) fn write_bytes_to_arg_buffer(&self, buf: &[u8]) -> u32 {
        self.with_arg_buffer(|arg_buffer| {
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
        let fun: TypedFunction<u32, i32> = self
            .instance
            .exports
            .get_typed_function(&self.store, method_name)?;

        self.set_remaining_points(limit);
        fun.call(&mut self.store, arg_len)
            .map_err(|e| map_call_err(self, e))
    }

    pub fn set_remaining_points(&mut self, limit: u64) {
        set_remaining_points(&mut self.store, &self.instance, limit);
    }

    pub fn get_remaining_points(&mut self) -> Option<u64> {
        match get_remaining_points(&mut self.store, &self.instance) {
            MeteringPoints::Remaining(points) => Some(points),
            MeteringPoints::Exhausted => None,
        }
    }

    pub fn is_function_exported<N: AsRef<str>>(&self, name: N) -> bool {
        self.instance.exports.get_function(name.as_ref()).is_ok()
    }

    #[allow(unused)]
    pub fn print_state(&self) {
        let mem = self
            .instance
            .exports
            .get_memory("memory")
            .expect("memory export is checked at contract creation time");

        let view = mem.view(&self.store);
        let maybe_interesting = unsafe { view.data_unchecked_mut() };

        const CSZ: usize = 128;
        const RSZ: usize = 16;

        for (chunk_nr, chunk) in maybe_interesting.chunks(CSZ).enumerate() {
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
                            print!("{}", format!("{byte:02x}").green());
                            print!(" ");
                        } else {
                            print!("{byte:02x} ")
                        }
                    }

                    println!();
                }
            }
        }
    }

    pub fn arg_buffer_offset(&self) -> usize {
        self.arg_buf_ofs
    }
}

fn map_call_err(instance: &mut WrappedInstance, err: RuntimeError) -> Error {
    if instance.get_remaining_points().is_none() {
        return Error::OutOfPoints;
    }

    err.into()
}

pub struct InstanceTunables {
    memory: Memory,
}

impl InstanceTunables {
    pub fn new(memory: Memory) -> Self {
        InstanceTunables { memory }
    }
}

impl Tunables for InstanceTunables {
    fn memory_style(&self, _memory: &MemoryType) -> MemoryStyle {
        self.memory.style()
    }

    fn table_style(&self, _table: &TableType) -> TableStyle {
        TableStyle::CallerChecksSignature
    }

    fn create_host_memory(
        &self,
        _ty: &MemoryType,
        _style: &MemoryStyle,
    ) -> Result<VMMemory, MemoryError> {
        Ok(VMMemory::from_custom(self.memory.clone()))
    }

    unsafe fn create_vm_memory(
        &self,
        _ty: &MemoryType,
        _style: &MemoryStyle,
        vm_definition_location: NonNull<wasmer_vm::VMMemoryDefinition>,
    ) -> Result<VMMemory, MemoryError> {
        // now, it's important to update vm_definition_location with the memory
        // information!
        let mut ptr = vm_definition_location;
        let md = ptr.as_mut();

        let memory = self.memory.clone();

        *md = *memory.vmmemory().as_ptr();

        Ok(memory.into())
    }

    /// Create a table owned by the host given a [`TableType`] and a
    /// [`TableStyle`].
    fn create_host_table(
        &self,
        ty: &TableType,
        style: &TableStyle,
    ) -> Result<VMTable, String> {
        VMTable::new(ty, style)
    }

    unsafe fn create_vm_table(
        &self,
        ty: &TableType,
        style: &TableStyle,
        vm_definition_location: NonNull<VMTableDefinition>,
    ) -> Result<VMTable, String> {
        VMTable::from_definition(ty, style, vm_definition_location)
    }
}

fn cost_function(_op: &Operator) -> u64 {
    1
}
