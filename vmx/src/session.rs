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

use uplink::ModuleId;

use crate::instance::WrappedInstance;
use crate::memory_handler::MemoryHandler;
use crate::types::{Error, StandardBufSerializer};
use crate::vm::VM;

#[derive(Clone)]
pub struct Session {
    vm: VM,
    memory_handler: MemoryHandler,
}

impl Session {
    pub fn new(vm: VM) -> Self {
        Session {
            memory_handler: MemoryHandler::new(vm.clone()),
            vm,
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
        let mut instance = self.instance(id);

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
        let mut instance = self.instance(id);

        let arg_len = instance.write_to_arg_buffer(arg)?;
        let ret_len = instance.transact(method_name, arg_len)?;

        instance.read_from_arg_buffer(ret_len)
    }

    pub fn commit(&self) -> VM {
        todo!()
    }

    pub(crate) fn instance(&self, mod_id: ModuleId) -> WrappedInstance {
        self.vm.with_module(mod_id, |module| {
            let memory = self.memory_handler.get_memory(mod_id);

            let fresh = memory.fresh();
            if !fresh {
                memory.save_volatile();
            }

            let wrapped = WrappedInstance::new(
                memory.clone(),
                self.clone(),
                mod_id,
                module,
            )
            .expect("todo, error handling");

            if !fresh {
                memory.restore_volatile();
            } else {
                memory.set_fresh(false);
            }

            wrapped
        })
    }
}
