// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fs;
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

use crate::commit::{CommitId, ModuleCommitId, SessionCommit};
use crate::event::Event;
use crate::instance::WrappedInstance;
use crate::memory_handler::MemoryHandler;
use crate::memory_path::MemoryPath;
use crate::module::WrappedModule;
use crate::types::MemoryFreshness::*;
use crate::types::StandardBufSerializer;
use crate::vm::VM;
use crate::Error::{self, CommitError};

const DEFAULT_LIMIT: u64 = 65_536;
const MAX_META_SIZE: usize = 65_536;

pub struct StackElementView<'a> {
    pub module_id: ModuleId,
    pub instance: &'a mut WrappedInstance,
    pub limit: u64,
}

struct StackElement {
    module_id: ModuleId,
    instance: *mut WrappedInstance,
    limit: u64,
}

impl StackElement {
    /// Creates a new stack element and __leaks__ the instance, returning a
    /// pointer to it.
    ///
    /// # Safety
    /// The instance will be re-acquired and dropped once the stack element is
    /// dropped. Any remaining pointers to the instance will be left dangling.
    /// It is up to the user to ensure the pointer and any aliases are only
    /// de-referenced for the lifetime of the element.
    pub fn new(
        module_id: ModuleId,
        instance: WrappedInstance,
        limit: u64,
    ) -> Self {
        let instance = Box::leak(Box::new(instance)) as *mut WrappedInstance;
        Self {
            module_id,
            instance,
            limit,
        }
    }

    fn instance<'a, 'b>(&'a self) -> &'b mut WrappedInstance {
        unsafe { &mut *self.instance }
    }
}

impl Drop for StackElement {
    fn drop(&mut self) {
        unsafe { Box::from_raw(self.instance) };
    }
}

unsafe impl Send for Session {}
unsafe impl Sync for Session {}

#[derive(Clone)]
pub struct Session {
    vm: VM,
    modules: BTreeMap<ModuleId, WrappedModule>,
    memory_handler: MemoryHandler,
    callstack: Arc<RwLock<Vec<StackElement>>>,
    debug: Arc<RwLock<Vec<String>>>,
    events: Arc<RwLock<Vec<Event>>>,
    data: Arc<RwLock<HostData>>,
    limit: u64,
    spent: u64,
}

impl Session {
    pub fn new(vm: VM) -> Self {
        Session {
            modules: BTreeMap::default(),
            memory_handler: MemoryHandler::new(vm.clone()),
            vm,
            callstack: Arc::new(RwLock::new(vec![])),
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

    fn with_module<F, R>(&self, id: ModuleId, closure: F) -> R
    where
        F: FnOnce(&WrappedModule) -> R,
    {
        let wrapped = self.modules.get(&id).expect("invalid module");

        closure(wrapped)
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
        let instance = self.new_instance(id);
        let instance = self.push_callstack(id, instance, self.limit).instance;

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
        arg: Arg,
    ) -> Result<Ret, Error>
    where
        Arg: for<'b> Serialize<StandardBufSerializer<'b>>,
        Ret: Archive,
        Ret::Archived: Deserialize<Ret, Infallible>
            + for<'b> CheckBytes<DefaultValidator<'b>>,
    {
        let instance = self.new_instance(id);
        let instance = self.push_callstack(id, instance, self.limit).instance;

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

    pub(crate) fn new_instance(&self, mod_id: ModuleId) -> WrappedInstance {
        self.with_module(mod_id, |module| {
            let mut memory = self
                .memory_handler
                .get_memory(mod_id)
                .expect("memory available");

            let wrapped = WrappedInstance::new(
                memory.clone(),
                self.clone(),
                mod_id,
                module,
            )
            .expect("todo, error handling");

            if memory.freshness() == Fresh {
                // if current commit exists, use it as memory image
                if let Some(commit_path) = self.path_to_current_commit(&mod_id)
                {
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
            memory.set_freshness(NotFresh);
            wrapped
        })
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

    pub(crate) fn nth_from_top<'a, 'b>(
        &'a self,
        n: usize,
    ) -> Option<StackElementView<'b>> {
        let stack = self.callstack.read();
        let len = stack.len();

        if len > n {
            let elem = &stack[len - (n + 1)];

            Some(StackElementView {
                module_id: elem.module_id,
                instance: elem.instance(),
                limit: elem.limit,
            })
        } else {
            None
        }
    }

    pub(crate) fn push_callstack<'a, 'b>(
        &'a self,
        module_id: ModuleId,
        instance: WrappedInstance,
        limit: u64,
    ) -> StackElementView<'b> {
        let mut s = self.callstack.write();

        let element = StackElement::new(module_id, instance, limit);
        let instance = element.instance();

        s.push(element);

        StackElementView {
            module_id,
            instance,
            limit,
        }
    }

    pub(crate) fn pop_callstack(&self) {
        let mut s = self.callstack.write();
        s.pop();
    }

    pub fn commit(mut self) -> Result<CommitId, Error> {
        let mut session_commit = SessionCommit::new();
        self.memory_handler.with_every_module_id(|module_id, mem| {
            let (source_path, _) = self.vm.memory_path(module_id);
            let module_commit_id = ModuleCommitId::from(mem)?;
            let target_path =
                self.vm.path_to_module_commit(module_id, &module_commit_id);
            let last_commit_path =
                self.vm.path_to_module_last_commit(module_id);
            std::fs::copy(source_path.as_ref(), target_path.as_ref())
                .map_err(CommitError)?;
            std::fs::copy(source_path.as_ref(), last_commit_path.as_ref())
                .map_err(CommitError)?;
            fs::remove_file(source_path.as_ref()).map_err(CommitError)?;
            session_commit.add(module_id, &module_commit_id);
            Ok(())
        })?;
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
