// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use bytecheck::CheckBytes;
use dallo::ModuleId;
use rkyv::{
    validation::validators::DefaultValidator, Archive, Deserialize, Infallible,
    Serialize,
};
use std::collections::BTreeMap;

use crate::instance::WrappedInstance;
use crate::types::{Error, Event, StandardBufSerializer};
use crate::vm::VM;

pub struct Session<'a> {
    vm: &'a VM,
    instances: BTreeMap<ModuleId, WrappedInstance>,
}

impl<'a> Session<'a> {
    pub(crate) fn new(vm: &'a VM) -> Self {
        Session {
            vm,
            instances: BTreeMap::new(),
        }
    }

    fn initialize_module(&mut self, id: ModuleId) -> Result<(), Error> {
        if self.instances.get(&id).is_some() {
            return Ok(());
        }
        let module = self.vm.module(id);
        let wrapped = WrappedInstance::new(module)?;
        self.instances.insert(id, wrapped);
        Ok(())
    }

    fn get_instance(
        &mut self,
        id: ModuleId,
    ) -> Result<&mut WrappedInstance, Error> {
        self.initialize_module(id)?;
        Ok(self.instances.get_mut(&id).expect("initialized above"))
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
        let i = self.get_instance(id)?;
        i.query(method_name, arg)
    }

    pub fn spent(&self) -> u64 {
        todo!()
    }

    pub fn set_limit(&mut self, limit: u64) {
        todo!()
    }

    pub fn debug(&self) -> &[String] {
        todo!()
    }

    pub fn set_height(&mut self, h: u64) {
        todo!()
    }
}

pub struct SessionMut<'a> {
    vm: &'a VM,
    instances: BTreeMap<ModuleId, WrappedInstance>,
}

impl<'a> SessionMut<'a> {
    pub(crate) fn new(vm: &'a VM) -> Self {
        SessionMut {
            vm,
            instances: BTreeMap::new(),
        }
    }

    fn initialize_module(&mut self, id: ModuleId) -> Result<(), Error> {
        if self.instances.get(&id).is_some() {
            return Ok(());
        }
        let module = self.vm.module(id);
        let wrapped = WrappedInstance::new(module)?;
        self.instances.insert(id, wrapped);
        Ok(())
    }

    fn get_instance(
        &mut self,
        id: ModuleId,
    ) -> Result<&mut WrappedInstance, Error> {
        self.initialize_module(id)?;
        Ok(self.instances.get_mut(&id).expect("initialized above"))
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
        let mut session = Session::new(self.vm);
        session.query(id, method_name, arg)
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
        let i = self.get_instance(id)?;
        i.transact(method_name, arg)
    }

    pub fn deploy(&mut self, bytecode: &[u8]) -> Result<ModuleId, Error> {
        todo!()
    }

    pub fn spent(&self) -> u64 {
        todo!()
    }

    pub fn set_limit(&mut self, limit: u64) {
        todo!()
    }

    pub fn debug(&self) -> &[String] {
        todo!()
    }

    pub fn events(&self) -> &[Event] {
        todo!()
    }

    pub fn set_height(&mut self, h: u64) {
        todo!()
    }
}
