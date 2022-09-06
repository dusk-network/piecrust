// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use bytecheck::CheckBytes;
use rkyv::{
    validation::validators::DefaultValidator, Archive, Deserialize, Infallible,
    Serialize,
};
use wasmer::{Store, Tunables};
use wasmer_compiler_singlepass::Singlepass;
use wasmer_vm::VMMemory;

use uplink::ModuleId;

use crate::instance::WrappedInstance;
use crate::linear::{Linear, MEMORY_PAGES};
use crate::memory_handler::MemoryHandler;
use crate::types::{Error, StandardBufSerializer};
use crate::vm::VM;

#[derive(Clone)]
pub struct Session {
    vm: VM,
    memory_handler: MemoryHandler,
}

pub struct SessionTunables {
    // memory: Linear,
}

impl SessionTunables {
    pub fn new() -> Self {
        SessionTunables {}
    }
}

impl Tunables for SessionTunables {
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
        let memory = Linear::new();
        Ok(VMMemory::from_custom(memory))
    }

    unsafe fn create_vm_memory(
        &self,
        _ty: &wasmer::MemoryType,
        _style: &wasmer_vm::MemoryStyle,
        vm_definition_location: std::ptr::NonNull<
            wasmer_vm::VMMemoryDefinition,
        >,
    ) -> Result<wasmer_vm::VMMemory, wasmer_vm::MemoryError> {
        let memory = Linear::new();
        // now, it's important to update vm_definition_location with the memory
        // information!
        let mut ptr = vm_definition_location;
        let md = ptr.as_mut();
        let unsafecell = memory.memory_definition.as_ref().unwrap();
        let def = unsafecell.get().as_ref().unwrap();
        md.base = def.base;
        md.current_length = def.current_length;
        Ok(memory.into())
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

impl Session {
    pub fn new(vm: VM) -> Self {
        Session {
            vm,
            memory_handler: MemoryHandler::new(),
        }
    }

    pub fn query<Arg, Ret>(
        &mut self,
        id: ModuleId,
        method_name: &str,
        arg: Arg,
    ) -> Result<Ret, Error>
    where
        Arg: for<'b> Serialize<StandardBufSerializer<'b>>,
        Ret: Archive,
        Ret::Archived: Deserialize<Ret, Infallible>
            + for<'b> CheckBytes<DefaultValidator<'b>>,
    {
        println!("query in session");
        let mut instance = self.instance(id);

        println!("instance created");

        let arg_len = instance.write_to_arg_buffer(arg)?;
        let ret_len = instance.query(method_name, arg_len)?;
        instance.read_from_arg_buffer(ret_len)
    }

    pub fn transact<Arg, Ret>(
        &mut self,
        id: ModuleId,
        method_name: &str,
        arg: Arg,
    ) -> Result<Ret, Error>
    where
        Arg: for<'b> Serialize<StandardBufSerializer<'b>>,
        Ret: Archive,
        Ret::Archived: Deserialize<Ret, Infallible>
            + for<'b> CheckBytes<DefaultValidator<'b>>,
    {
        println!("in session transact");
        let mut instance = self.instance(id);
        instance.transact(method_name, arg)
    }

    pub fn commit(&self) -> VM {
        todo!()
    }

    pub(crate) fn instance(&self, mod_id: ModuleId) -> WrappedInstance {
        println!("request instance");

        self.vm.with_module(mod_id, |serialized_module| {
            println!("with module");

            let memory = self.memory_handler.get_memory(mod_id);

            println!("memory aquired");

            let store = Store::new_with_tunables(
                Singlepass::default(),
                SessionTunables { // memory 
		},
            );

            println!("store created");

            WrappedInstance::new(store, self.clone(), mod_id, serialized_module)
                .expect("todo")
        })
    }
}
