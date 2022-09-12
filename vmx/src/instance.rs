// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::ops::{Deref, DerefMut};

use bytecheck::CheckBytes;
use colored::*;
use rkyv::{
    check_archived_root,
    ser::{
        serializers::{BufferScratch, BufferSerializer, CompositeSerializer},
        Serializer,
    },
    validation::validators::DefaultValidator,
    Archive, Deserialize, Infallible, Serialize,
};
use uplink::{ModuleId, SCRATCH_BUF_BYTES};
use wasmer::{Store, Tunables, TypedFunction};
use wasmer_compiler_singlepass::Singlepass;
use wasmer_vm::VMMemory;

use crate::imports::DefaultImports;
use crate::linear::{Linear, MEMORY_PAGES};
use crate::module::WrappedModule;
use crate::session::Session;
use crate::types::{Error, StandardBufSerializer};

pub struct WrappedInstance {
    instance: wasmer::Instance,
    arg_buf_ofs: usize,
    heap_base: usize,
    store: wasmer::Store,
}

pub(crate) struct Env {
    self_id: ModuleId,
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
    pub fn self_instance(&self) -> WrappedInstance {
        self.session.instance(self.self_id)
    }
}

impl WrappedInstance {
    pub fn new(
        memory: Linear,
        // mut store: wasmer::Store,
        session: Session,
        id: ModuleId,
        wrap: &WrappedModule,
    ) -> Result<Self, Error> {
        println!("in wrapped instance new");

        let mut store = Store::new_with_tunables(
            Singlepass::default(),
            InstanceTunables::new(memory.clone()),
        );

        println!("store created");

        let env = Env {
            self_id: id,
            session: session.clone(),
        };

        let imports = DefaultImports::default(&mut store, env);
        let module_bytes = wrap.as_bytes();

        let module =
            unsafe { wasmer::Module::deserialize(&store, module_bytes)? };

        println!("pre instance creation");

        let instance = wasmer::Instance::new(&mut store, &module, &imports)?;

        println!("post instance creation");

        let arg_buf_ofs =
            match instance.exports.get_global("A")?.get(&mut store) {
                wasmer::Value::I32(i) => i as usize,
                _ => todo!("Missing `A` Argbuf export"),
            };

        let heap_base =
            match instance.exports.get_global("__heap_base")?.get(&mut store) {
                wasmer::Value::I32(i) => i as usize,
                _ => todo!("Missing heap base"),
            };

        let wrapped = WrappedInstance {
            store,
            instance,
            arg_buf_ofs,
            heap_base,
        };

        wrapped.snap();

        Ok(wrapped)
    }

    pub(crate) fn copy_argument(&mut self, arg: &[u8]) {
        self.with_arg_buffer(|buf| buf[..arg.len()].copy_from_slice(arg))
    }

    pub(crate) fn read_from_arg_buffer<T>(
        &self,
        arg_len: u32,
    ) -> Result<T, Error>
    where
        T: Archive,
        T::Archived: Deserialize<T, Infallible>
            + for<'b> CheckBytes<DefaultValidator<'b>>,
    {
        // TODO use bytecheck here
        self.with_arg_buffer(|abuf| {
            let slice = &abuf[..arg_len as usize];
            let ta: &T::Archived = check_archived_root::<T>(slice)?;
            let t = ta.deserialize(&mut Infallible).expect("Infallible");
            Ok(t)
        })
    }

    pub(crate) fn with_memory<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        let mem =
            self.instance.exports.get_memory("memory").expect(
                "memory export should be checked at module creation time",
            );
        let view = mem.view(&self.store);
        let memory_bytes = unsafe { view.data_unchecked() };
        f(memory_bytes)
    }

    fn with_memory_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mem =
            self.instance.exports.get_memory("memory").expect(
                "memory export should be checked at module creation time",
            );
        let view = mem.view(&self.store);
        let memory_bytes = unsafe { view.data_unchecked_mut() };
        f(memory_bytes)
    }

    pub(crate) fn with_arg_buffer<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        self.with_memory_mut(|memory_bytes| {
            let a = self.arg_buf_ofs;
            let b = uplink::ARGBUF_LEN;
            let begin = &mut memory_bytes[a..];
            let trimmed = &mut begin[..b];
            f(trimmed)
        })
    }

    pub(crate) fn write_to_arg_buffer<T>(&self, value: T) -> Result<u32, Error>
    where
        T: for<'b> Serialize<StandardBufSerializer<'b>>,
    {
        self.with_arg_buffer(|abuf| {
            let mut sbuf = [0u8; SCRATCH_BUF_BYTES];
            let scratch = BufferScratch::new(&mut sbuf);
            let ser = BufferSerializer::new(abuf);
            let mut ser =
                CompositeSerializer::new(ser, scratch, rkyv::Infallible);

            ser.serialize_value(&value)?;

            Ok(ser.pos() as u32)
        })
    }

    pub fn query(
        &mut self,
        method_name: &str,
        arg_len: u32,
    ) -> Result<u32, Error> {
        let fun: TypedFunction<u32, u32> = self
            .instance
            .exports
            .get_typed_function(&self.store, method_name)?;

        Ok(fun.call(&mut self.store, arg_len)?)
    }

    pub fn transact(
        &mut self,
        method_name: &str,
        arg_len: u32,
    ) -> Result<u32, Error> {
        let fun: TypedFunction<u32, u32> = self
            .instance
            .exports
            .get_typed_function(&self.store, method_name)?;

        Ok(fun.call(&mut self.store, arg_len)?)
    }

    pub fn snap(&self) {
        let mem = self
            .instance
            .exports
            .get_memory("memory")
            .expect("memory export is checked at module creation time");

        println!("memory snapshot");

        let view = mem.view(&self.store);
        let maybe_interesting = unsafe { view.data_unchecked_mut() };

        const CSZ: usize = 128;
        const RSZ: usize = 16;

        for (chunk_nr, chunk) in maybe_interesting.chunks(CSZ).enumerate() {
            if chunk[..] != [0; CSZ][..] {
                for (row_nr, row) in chunk.chunks(RSZ).enumerate() {
                    let ofs = chunk_nr * CSZ + row_nr * RSZ;

                    print!("{:08x}:", ofs);

                    for (i, byte) in row.iter().enumerate() {
                        if i % 4 == 0 {
                            print!(" ");
                        }

                        let buf_start = self.arg_buf_ofs as usize;
                        let buf_end = buf_start + uplink::ARGBUF_LEN as usize;
                        let heap_base = self.heap_base as usize;

                        if ofs + i >= buf_start && ofs + i < buf_end {
                            print!("{}", format!("{:02x}", byte).red());
                            print!(" ");
                        } else if ofs + i >= heap_base {
                            print!("{}", format!("{:02x} ", byte).green());
                        } else {
                            print!("{:02x} ", byte)
                        }
                    }

                    println!();
                }
            }
        }
    }
}

pub struct InstanceTunables {
    memory: Linear,
}

impl InstanceTunables {
    pub fn new(memory: Linear) -> Self {
        InstanceTunables { memory }
    }
}

impl Tunables for InstanceTunables {
    fn memory_style(
        &self,
        _memory: &wasmer::MemoryType,
    ) -> wasmer_vm::MemoryStyle {
        wasmer_vm::MemoryStyle::Static {
            bound: wasmer::Pages::from(MEMORY_PAGES as u32),
            offset_guard_size: 0,
        }
    }

    fn table_style(&self, _table: &wasmer::TableType) -> wasmer_vm::TableStyle {
        wasmer_vm::TableStyle::CallerChecksSignature
    }

    fn create_host_memory(
        &self,
        _ty: &wasmer::MemoryType,
        _style: &wasmer_vm::MemoryStyle,
    ) -> Result<wasmer_vm::VMMemory, wasmer_vm::MemoryError> {
        Ok(VMMemory::from_custom(self.memory.clone()))
    }

    unsafe fn create_vm_memory(
        &self,
        _ty: &wasmer::MemoryType,
        _style: &wasmer_vm::MemoryStyle,
        vm_definition_location: std::ptr::NonNull<
            wasmer_vm::VMMemoryDefinition,
        >,
    ) -> Result<wasmer_vm::VMMemory, wasmer_vm::MemoryError> {
        // now, it's important to update vm_definition_location with the memory
        // information!
        let mut ptr = vm_definition_location;
        let md = ptr.as_mut();

        let mem = self.memory.clone();

        *md = mem.definition();

        Ok(mem.into())
    }

    /// Create a table owned by the host given a [`TableType`] and a
    /// [`TableStyle`].
    fn create_host_table(
        &self,
        ty: &wasmer::TableType,
        style: &wasmer_vm::TableStyle,
    ) -> Result<wasmer_vm::VMTable, String> {
        wasmer_vm::VMTable::new(ty, style)
    }

    unsafe fn create_vm_table(
        &self,
        ty: &wasmer::TableType,
        style: &wasmer_vm::TableStyle,
        vm_definition_location: std::ptr::NonNull<wasmer_vm::VMTableDefinition>,
    ) -> Result<wasmer_vm::VMTable, String> {
        wasmer_vm::VMTable::from_definition(ty, style, vm_definition_location)
    }
}
