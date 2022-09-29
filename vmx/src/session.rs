// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::fs;
use std::sync::Arc;

use bytecheck::CheckBytes;
use parking_lot::RwLock;
use rkyv::{
    validation::validators::DefaultValidator, Archive, Deserialize, Infallible,
    Serialize,
};

use uplink::ModuleId;

use crate::commit::{CommitId, ModuleCommitId, SessionCommit};
use crate::event::Event;
use crate::instance::WrappedInstance;
use crate::memory_handler::MemoryHandler;
use crate::memory_path::MemoryPath;
use crate::types::MemoryFreshness::*;
use crate::types::StandardBufSerializer;
use crate::vm::VM;
use crate::Error::{self, CommitError};

#[derive(Clone)]
pub struct Session {
    vm: VM,
    memory_handler: MemoryHandler,
    callstack: Arc<RwLock<Vec<ModuleId>>>,
    debug: Arc<RwLock<Vec<String>>>,
    events: Arc<RwLock<Vec<Event>>>,
}

impl Session {
    pub fn new(vm: VM) -> Self {
        Session {
            memory_handler: MemoryHandler::new(vm.clone()),
            vm,
            callstack: Arc::new(RwLock::new(vec![])),
            debug: Arc::new(RwLock::new(vec![])),
            events: Arc::new(RwLock::new(vec![])),
        }
    }

    pub fn query<Arg, Ret>(
        &self,
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

    pub(crate) fn instance(&self, mod_id: ModuleId) -> WrappedInstance {
        self.vm.with_module(mod_id, |module| {
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

    pub fn nth_from_top(&self, n: usize) -> ModuleId {
        let stack = self.callstack.read();
        let len = stack.len();

        if len > n + 1 {
            stack[len - (n + 1)]
        } else {
            ModuleId::uninitialized()
        }
    }

    pub(crate) fn push_callstack(&self, id: ModuleId) {
        let mut s = self.callstack.write();
        s.push(id);
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

    pub fn set_meta<V>(&self, _name: &str, _value: V) {
        todo!()
    }
}
