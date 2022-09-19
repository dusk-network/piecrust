// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::collections::BTreeMap;
use std::sync::Arc;

use bytecheck::CheckBytes;
use parking_lot::RwLock;
use rkyv::{
    validation::validators::DefaultValidator, Archive, Deserialize, Infallible,
    Serialize,
};

use uplink::ModuleId;

use crate::module::WrappedModule;
use crate::session::Session;
use crate::types::{Error, StandardBufSerializer};

#[derive(Default)]
struct VMInner {
    modules: BTreeMap<ModuleId, WrappedModule>,
}

#[derive(Clone)]
pub struct VM {
    inner: Arc<RwLock<VMInner>>,
}

impl Default for VM {
    fn default() -> VM {
        VM {
            inner: Arc::new(RwLock::new(VMInner::default())),
        }
    }
}

impl VM {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn deploy(&mut self, bytecode: &[u8]) -> Result<ModuleId, Error> {
        // This should be the only place that we need a write lock.
        let mut guard = self.inner.write();
        let hash = blake3::hash(bytecode);
        let id = ModuleId::from(<[u8; 32]>::from(hash));

        let module = WrappedModule::new(bytecode)?;
        guard.modules.insert(id, module);
        Ok(id)
    }

    pub fn with_memory<F, R>(&self, _id: ModuleId, _closure: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        todo!()
    }

    pub fn with_argbuf<F, R>(&self, _id: ModuleId, _closure: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        todo!()
    }

    pub fn with_module<F, R>(&self, id: ModuleId, closure: F) -> R
    where
        F: FnOnce(&WrappedModule) -> R,
    {
        let guard = self.inner.read();
        let wrapped = guard.modules.get(&id).expect("invalid module");

        closure(wrapped)
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
        let mut session = Session::new(self.clone());
        session.query(id, method_name, arg)
    }

    pub fn session(&mut self) -> Session {
        Session::new(self.clone())
    }
}
