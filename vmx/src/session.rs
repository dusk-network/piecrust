// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::sync::Arc;

use bytecheck::CheckBytes;
use parking_lot::RwLock;
use rand::prelude::*;
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
use crate::Error::{self, CommitError};

pub const COMMIT_ID_BYTES: usize = 4;

#[derive(Clone, Copy, PartialOrd, Ord, PartialEq, Eq)]
pub struct CommitId([u8; COMMIT_ID_BYTES]);

impl CommitId {
    pub fn new() -> CommitId {
        CommitId(thread_rng().gen::<[u8; COMMIT_ID_BYTES]>())
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.0[..]
    }
}

impl Default for CommitId {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Debug for CommitId {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        if f.alternate() {
            write!(f, "0x")?
        }
        for byte in self.0 {
            write!(f, "{:02x}", &byte)?
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct Session {
    vm: VM,
    memory_handler: MemoryHandler,
    callstack: Arc<RwLock<Vec<ModuleId>>>,
}

impl Session {
    pub fn new(vm: VM) -> Self {
        Session {
            memory_handler: MemoryHandler::new(vm.clone()),
            vm,
            callstack: Arc::new(RwLock::new(vec![])),
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

    pub fn commit(&mut self, id: &ModuleId) -> Result<CommitId, Error> {
        let commit_id = CommitId::new();
        let (source_path, _) = self.vm.module_memory_path(id);
        let target_path = self.vm.commit(id, &commit_id);
        std::fs::copy(source_path.as_ref(), target_path.as_ref())
            .map_err(CommitError)?;
        Ok(commit_id)
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
}
