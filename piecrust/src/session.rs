// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

pub mod call_stack;

use std::borrow::Cow;
use std::collections::BTreeMap;

use bytecheck::CheckBytes;
use piecrust_uplink::{ModuleId, SCRATCH_BUF_BYTES};
use rkyv::ser::serializers::{BufferScratch, BufferSerializer};
use rkyv::ser::Serializer;
use rkyv::{
    validation::validators::DefaultValidator, Archive, Deserialize, Infallible,
    Serialize,
};

use crate::event::Event;
use crate::instance::WrappedInstance;
use crate::module::WrappedModule;
use crate::store::ModuleSession;
use crate::types::StandardBufSerializer;
use crate::vm::HostQueries;
use crate::Error;
use crate::Error::PersistenceError;

use call_stack::{CallStack, StackElementView};

const DEFAULT_LIMIT: u64 = 65_536;
const MAX_META_SIZE: usize = 65_536;

unsafe impl Send for Session {}
unsafe impl Sync for Session {}

pub struct Session {
    callstack: CallStack,
    debug: Vec<String>,
    events: Vec<Event>,
    data: Metadata,

    module_session: ModuleSession,
    host_queries: HostQueries,

    limit: u64,
    spent: u64,
}

impl Session {
    pub(crate) fn new(
        module_session: ModuleSession,
        host_queries: HostQueries,
    ) -> Self {
        Session {
            callstack: CallStack::new(),
            debug: vec![],
            events: vec![],
            data: Metadata::new(),
            module_session,
            host_queries,
            limit: DEFAULT_LIMIT,
            spent: 0,
        }
    }

    /// Deploy a module, returning its `ModuleId`. The ID is computed using a
    /// `blake3` hash of the bytecode.
    ///
    /// If one needs to specify the ID, [`deploy_with_id`] is available.
    ///
    /// [`deploy_with_id`]: `Session::deploy_with_id`
    pub fn deploy(&mut self, bytecode: &[u8]) -> Result<ModuleId, Error> {
        let module_id = self
            .module_session
            .deploy(bytecode)
            .map_err(PersistenceError)?;

        Ok(module_id)
    }

    /// Deploy a module with the given ID.
    ///
    /// If one would like to *not* specify the `ModuleId`, [`deploy`] is
    /// available.
    ///
    /// [`deploy`]: `Session::deploy`
    pub fn deploy_with_id(
        &mut self,
        module_id: ModuleId,
        bytecode: &[u8],
    ) -> Result<(), Error> {
        self.module_session
            .deploy_with_id(module_id, bytecode)
            .map_err(PersistenceError)?;
        Ok(())
    }

    pub fn query<Arg, Ret>(
        &mut self,
        id: ModuleId,
        method_name: &str,
        arg: &Arg,
    ) -> Result<Ret, Error>
    where
        Arg: for<'b> Serialize<StandardBufSerializer<'b>>,
        Ret: Archive,
        Ret::Archived: Deserialize<Ret, Infallible>
            + for<'b> CheckBytes<DefaultValidator<'b>>,
    {
        let instance = self.push_callstack(id, self.limit)?.instance;

        let ret = {
            let arg_len = instance.write_to_arg_buffer(arg)?;
            let ret_len = instance.query(method_name, arg_len, self.limit)?;
            instance.read_from_arg_buffer(ret_len)
        };

        self.spent = self.limit
            - instance
                .get_remaining_points()
                .expect("there should be remaining points");

        self.pop_callstack();

        ret
    }

    pub fn transact<Arg, Ret>(
        &mut self,
        id: ModuleId,
        method_name: &str,
        arg: &Arg,
    ) -> Result<Ret, Error>
    where
        Arg: for<'b> Serialize<StandardBufSerializer<'b>>,
        Ret: Archive,
        Ret::Archived: Deserialize<Ret, Infallible>
            + for<'b> CheckBytes<DefaultValidator<'b>>,
    {
        let instance = self.push_callstack(id, self.limit)?.instance;

        let ret = {
            let arg_len = instance.write_to_arg_buffer(arg)?;
            let ret_len =
                instance.transact(method_name, arg_len, self.limit)?;
            instance.read_from_arg_buffer(ret_len)
        };

        self.spent = self.limit
            - instance
                .get_remaining_points()
                .expect("there should be remaining points");

        self.pop_callstack();

        ret
    }

    pub fn root(&self) -> [u8; 32] {
        self.module_session.root()
    }

    pub(crate) fn push_event(&mut self, event: Event) {
        self.events.push(event);
    }

    fn new_instance(
        &mut self,
        module_id: ModuleId,
    ) -> Result<WrappedInstance, Error> {
        let (bytecode, memory) = self
            .module_session
            .module(module_id)
            .map_err(PersistenceError)?
            .expect("Module should exist");

        let module = WrappedModule::new(&bytecode)?;
        let instance = WrappedInstance::new(self, module_id, module, memory)?;

        Ok(instance)
    }

    pub(crate) fn host_query(
        &self,
        name: &str,
        buf: &mut [u8],
        arg_len: u32,
    ) -> Option<u32> {
        self.host_queries.call(name, buf, arg_len)
    }

    /// Sets the point limit for the next call to `query` or `transact`.
    pub fn set_point_limit(&mut self, limit: u64) {
        self.limit = limit
    }

    pub fn spent(&self) -> u64 {
        self.spent
    }

    pub(crate) fn nth_from_top<'a>(
        &self,
        n: usize,
    ) -> Option<StackElementView<'a>> {
        self.callstack.nth_from_top(n)
    }

    pub(crate) fn push_callstack<'b>(
        &mut self,
        module_id: ModuleId,
        limit: u64,
    ) -> Result<StackElementView<'b>, Error> {
        let instance = self.callstack.instance(&module_id);

        match instance {
            Some(_) => {
                self.callstack.push(module_id, limit);
            }
            None => {
                let instance = self.new_instance(module_id)?;
                self.callstack.push_instance(module_id, limit, instance);
            }
        }

        Ok(self
            .callstack
            .nth_from_top(0)
            .expect("We just pushed an element to the stack"))
    }

    pub(crate) fn pop_callstack(&mut self) {
        self.callstack.pop();
    }

    pub fn commit(self) -> Result<[u8; 32], Error> {
        self.module_session.commit().map_err(PersistenceError)
    }

    pub(crate) fn register_debug<M: Into<String>>(&mut self, msg: M) {
        self.debug.push(msg.into());
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        core::mem::take(&mut self.events)
    }

    pub fn with_debug<C, R>(&self, c: C) -> R
    where
        C: FnOnce(&[String]) -> R,
    {
        c(&self.debug)
    }

    pub fn meta(&self, name: &str) -> Option<Vec<u8>> {
        self.data.get(name)
    }

    pub fn set_meta<S, V>(&mut self, name: S, value: V)
    where
        S: Into<Cow<'static, str>>,
        V: for<'a> Serialize<StandardBufSerializer<'a>>,
    {
        let mut buf = [0u8; MAX_META_SIZE];
        let mut sbuf = [0u8; SCRATCH_BUF_BYTES];

        let ser = BufferSerializer::new(&mut buf[..]);
        let scratch = BufferScratch::new(&mut sbuf);

        let mut serializer =
            StandardBufSerializer::new(ser, scratch, Infallible);
        serializer.serialize_value(&value).expect("Infallible");

        let pos = serializer.pos();

        let data = buf[..pos].to_vec();
        self.data.insert(name, data);
    }
}

#[derive(Debug)]
pub struct Metadata {
    data: BTreeMap<Cow<'static, str>, Vec<u8>>,
}

impl Metadata {
    fn new() -> Self {
        Self {
            data: BTreeMap::new(),
        }
    }

    fn insert<S>(&mut self, name: S, data: Vec<u8>)
    where
        S: Into<Cow<'static, str>>,
    {
        self.data.insert(name.into(), data);
    }

    fn get(&self, name: &str) -> Option<Vec<u8>> {
        self.data.get(name).cloned()
    }
}
