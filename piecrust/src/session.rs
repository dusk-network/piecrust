// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

pub mod call_stack;
use call_stack::{CallStack, StackElementView};

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;

use bytecheck::CheckBytes;
use parking_lot::RwLock;
use piecrust_uplink::{ModuleId, SCRATCH_BUF_BYTES};
use rkyv::ser::serializers::{BufferScratch, BufferSerializer};
use rkyv::ser::Serializer;
use rkyv::{
    validation::validators::DefaultValidator, Archive, Deserialize, Infallible,
    Serialize,
};

use crate::commit::{CommitId, SessionCommit, ModuleCommitStore};
use crate::event::Event;
use crate::instance::WrappedInstance;
use crate::memory_handler::MemoryHandler;
use crate::memory_path::MemoryPath;
use crate::module::WrappedModule;
use crate::types::MemoryState;
use crate::types::StandardBufSerializer;
use crate::vm::VM;
use crate::Error;

const DEFAULT_LIMIT: u64 = 65_536;
const MAX_META_SIZE: usize = 65_536;

unsafe impl<'c> Send for Session<'c> {}
unsafe impl<'c> Sync for Session<'c> {}

pub struct Session<'c> {
    vm: &'c mut VM,
    modules: BTreeMap<ModuleId, WrappedModule>,
    memory_handler: MemoryHandler,
    callstack: Arc<RwLock<CallStack>>,
    debug: Arc<RwLock<Vec<String>>>,
    events: Arc<RwLock<Vec<Event>>>,
    data: Arc<RwLock<HostData>>,
    limit: u64,
    spent: u64,
}

impl<'c> Session<'c> {
    pub fn new(vm: &'c mut VM) -> Self {
        let base_path = vm.base_path();
        Session {
            vm,
            modules: BTreeMap::default(),
            memory_handler: MemoryHandler::new(base_path),
            callstack: Arc::new(RwLock::new(CallStack::new())),
            debug: Arc::new(RwLock::new(vec![])),
            events: Arc::new(RwLock::new(vec![])),
            data: Arc::new(RwLock::new(HostData::new())),
            limit: DEFAULT_LIMIT,
            spent: 0,
        }
    }

    pub fn deploy(&mut self, bytecode: &[u8]) -> Result<ModuleId, Error> {
        let hash = blake3::hash(bytecode);
        let module_id = ModuleId::from(<[u8; 32]>::from(hash));
        self.deploy_with_id(module_id, bytecode)?;
        Ok(module_id)
    }

    pub fn deploy_with_id(
        &mut self,
        module_id: ModuleId,
        bytecode: &[u8],
    ) -> Result<(), Error> {
        let module = WrappedModule::new(bytecode)?;
        self.modules.insert(module_id, module);
        Ok(())
    }

    pub(crate) fn get_module(&self, id: ModuleId) -> &WrappedModule {
        self.modules.get(&id).expect("invalid module")
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
        let instance = self.push_callstack(id, self.limit).instance;

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
        let instance = self.push_callstack(id, self.limit).instance;

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

    pub(crate) fn push_event(&mut self, event: Event) {
        let mut events = self.events.write();
        events.push(event);
    }

    fn new_instance(&mut self, mod_id: ModuleId) -> WrappedInstance {
        let mut memory = self
            .memory_handler
            .get_memory(mod_id)
            .expect("memory available");

        let wrapped = WrappedInstance::new(memory.clone(), self, mod_id)
            .expect("todo, error handling");

        if memory.state() == MemoryState::Uninitialized {
            // if current commit exists, use it as memory image
            if let Some(commit_path) = self.path_to_current_commit(&mod_id) {
                let metadata = std::fs::metadata(commit_path.as_ref())
                    .expect("todo - metadata error handling");
                memory
                    .grow_to(metadata.len() as u32)
                    .expect("todo - grow error handling");
                let (target_path, _) = self.vm.memory_path(&mod_id);
                std::fs::copy(commit_path.as_ref(), target_path.as_ref())
                    .expect("commit and memory paths exist");
            }
        }
        memory.set_state(MemoryState::Initialized);
        wrapped
    }

    pub(crate) fn host_query(
        &self,
        name: &str,
        buf: &mut [u8],
        arg_len: u32,
    ) -> Option<u32> {
        self.vm.host_query(name, buf, arg_len)
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
        let stack = self.callstack.read();
        stack.nth_from_top(n)
    }

    pub(crate) fn push_callstack<'b>(
        &mut self,
        module_id: ModuleId,
        limit: u64,
    ) -> StackElementView<'b> {
        let s = self.callstack.write();
        let instance = s.instance(&module_id);

        drop(s);

        match instance {
            Some(_) => {
                let mut s = self.callstack.write();
                s.push(module_id, limit);
            }
            None => {
                let instance = self.new_instance(module_id);
                let mut s = self.callstack.write();
                s.push_instance(module_id, limit, instance);
            }
        }

        let s = self.callstack.write();
        s.nth_from_top(0)
            .expect("We just pushed an element to the stack")
    }

    pub(crate) fn pop_callstack(&self) {
        let mut s = self.callstack.write();
        s.pop();
    }

    pub fn commit(self) -> Result<CommitId, Error> {
        let mut session_commit = SessionCommit::new();
        self.memory_handler.with_every_module_id(|module_id, mem| {
            let module_commit_store = ModuleCommitStore::new(self.vm.base_path(), *module_id);
            let module_commit_id = module_commit_store.commit(mem)?;
            self.vm.reset_root();
            session_commit.add(module_id, &module_commit_id);
            Ok(())
        })?;
        session_commit.calculate_id();
        let session_commit_id = session_commit.commit_id();
        self.vm.add_session_commit(session_commit);
        Ok(session_commit_id)
    }

    pub fn restore(
        &mut self,
        session_commit_id: &CommitId,
    ) -> Result<(), Error> {
        self.vm.restore_session(session_commit_id)?;
        Ok(())
    }

    fn path_to_current_commit(
        &self,
        module_id: &ModuleId,
    ) -> Option<MemoryPath> {
        let path = self.vm.path_to_module_last_commit(module_id);
        Some(path).filter(|p| p.as_ref().exists())
    }

    pub(crate) fn register_debug<M: Into<String>>(&self, msg: M) {
        self.debug.write().push(msg.into());
    }

    pub fn take_events(&self) -> Vec<Event> {
        core::mem::take(&mut *self.events.write())
    }

    pub fn with_debug<C, R>(&self, c: C) -> R
    where
        C: FnOnce(&[String]) -> R,
    {
        c(&self.debug.read())
    }

    pub fn meta(&self, name: &str) -> Option<Vec<u8>> {
        let host_data = self.data.read();
        host_data.get(name)
    }

    pub fn set_meta<S, V>(&mut self, name: S, value: V)
    where
        S: Into<Cow<'static, str>>,
        V: for<'a> Serialize<StandardBufSerializer<'a>>,
    {
        let mut host_data = self.data.write();

        let mut buf = [0u8; MAX_META_SIZE];
        let mut sbuf = [0u8; SCRATCH_BUF_BYTES];

        let ser = BufferSerializer::new(&mut buf[..]);
        let scratch = BufferScratch::new(&mut sbuf);

        let mut serializer =
            StandardBufSerializer::new(ser, scratch, Infallible);
        serializer.serialize_value(&value).expect("Infallible");

        let pos = serializer.pos();

        let data = buf[..pos].to_vec();
        host_data.insert(name, data);
    }

    pub fn root(&mut self, refresh: bool) -> Result<[u8; 32], Error> {
        self.vm.root(refresh)
    }
}

struct HostData {
    data: BTreeMap<Cow<'static, str>, Vec<u8>>,
}

impl HostData {
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
