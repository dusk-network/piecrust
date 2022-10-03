// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::sync::Arc;

use bytecheck::CheckBytes;
use parking_lot::RwLock;
use rkyv::{
    validation::validators::DefaultValidator, Archive, Deserialize, Infallible,
    Serialize,
};

use uplink::ModuleId;

use crate::instance::WrappedInstance;
use crate::memory_handler::MemoryHandler;
use crate::types::MemoryFreshness::*;
use crate::types::StandardBufSerializer;
use crate::vm::VM;
use crate::Error::{self, CommitError, RestoreError, SessionError};
use crate::commit::{SessionCommits, SessionCommitId, SessionCommit, ModuleCommitId};
use crate::vm::MemoryPath;


#[derive(Clone)]
pub struct Session {
    vm: VM,
    memory_handler: MemoryHandler,
    callstack: Arc<RwLock<Vec<ModuleId>>>,
    current_commit_id: Option<SessionCommitId>,
}

impl Session {
    pub fn new(vm: VM) -> Self {
        Session {
            memory_handler: MemoryHandler::new(vm.clone()),
            vm,
            callstack: Arc::new(RwLock::new(vec![])),
            current_commit_id: None,
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

    pub(crate) fn instance(&self, mod_id: ModuleId) -> WrappedInstance {
        self.vm.with_module(mod_id, |module| {
            let memory = self
                .memory_handler
                .get_memory(mod_id)
                .expect("memory available");

            let freshness = memory.freshness();
            if freshness == NotFresh {
                memory.save_volatile();
            }

            let wrapped = WrappedInstance::new(
                memory.clone(),
                self.clone(),
                mod_id,
                module,
            )
            .expect("todo, error handling");

            if freshness == NotFresh {
                memory.restore_volatile();
            } else {
                memory.set_freshness(NotFresh);
            }

            // if current commit exists, use it as memory image
            // todo - this part does not work yet
            if let Some(commit_path) = self.path_to_current_commit(&mod_id) {
                let (target_path, _) = self.vm.memory_path(&mod_id);
                std::fs::copy(commit_path.as_ref(), target_path.as_ref())
                    .expect("commit and memory paths exist");
                memory.set_freshness(NotFresh)
            }
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

    pub(crate) fn push_callstack(&mut self, id: ModuleId) {
        let mut s = self.callstack.write();
        s.push(id);
    }

    pub(crate) fn pop_callstack(&mut self) {
        let mut s = self.callstack.write();
        s.pop();
    }

    pub fn commit(&mut self) -> Result<SessionCommitId, Error> {
        let mut session_commit = SessionCommit::new();
        let module_ids: Vec<ModuleId> = self.memory_handler.get_module_ids();
        for module_id in module_ids.iter() {
            let module_commit_id = ModuleCommitId::new();
            let (source_path, _) = self.vm.memory_path(module_id);
            let target_path = self.vm.path_to_commit(module_id, &module_commit_id);
            std::fs::copy(source_path.as_ref(), target_path.as_ref())
                .map_err(CommitError)?;
            session_commit.add(module_id, &module_commit_id);
        }
        self.set_current_commit(&session_commit.commit_id());
        println!("adding session commit {:?}", session_commit.commit_id());
        let session_commit_id = session_commit.commit_id();
        self.vm.add_session_commit(session_commit);
        Ok(session_commit_id)
    }

    pub fn restore(
        &mut self,
        session_commit_id: &SessionCommitId
    ) -> Result<(), Error> {
        println!("getting session commit {:?}", session_commit_id);
        match self.vm.get_session_commit(&session_commit_id) {
            Some(session_commit) => {
                for (module_id, module_commit_id) in session_commit.ids().iter() {
                    let source_path = self.vm.path_to_commit(module_id, module_commit_id);
                    let (target_path, _) = self.vm.memory_path(module_id);
                    std::fs::copy(source_path.as_ref(), target_path.as_ref())
                        .map_err(RestoreError)?;
                }
                self.set_current_commit(session_commit_id);
                Ok(())
            },
            None => Err(SessionError("unknown session commit id".to_string())),
        }
    }

    // todo: refactor this method or possibly eliminate
    pub fn path_to_current_commit(
        &self,
        module_id: &ModuleId,
    ) -> Option<MemoryPath> {
        if self.current_commit_id.is_none(){
            return None;
        }
        if let Some(session_commit_id) = self.current_commit_id {
            if let Some(session_commit) = self.vm.get_session_commit(&session_commit_id) {
                return session_commit.get(module_id).map(|module_commit_id|self.vm.path_to_commit(module_id, module_commit_id));
            }
        }
        None
    }

    // todo: refactor this method or possibly eliminate
    pub fn set_current_commit(
        &mut self,
        session_commit_id: &SessionCommitId,
    ) {
        self.current_commit_id = Some(*session_commit_id);
    }
}
