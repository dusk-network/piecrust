// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::collections::BTreeMap;

use bytecheck::CheckBytes;
use rkyv::{
    validation::validators::DefaultValidator, Archive, Deserialize, Infallible,
    Serialize,
};

use crate::module::WrappedModule;
use crate::session::{Session, SessionMut};
use crate::types::{Error, StandardBufSerializer};

#[derive(Clone, Copy, PartialOrd, Ord, PartialEq, Eq)]
pub struct ModuleId(usize);

#[derive(Default)]
pub struct VM {
    modules: BTreeMap<ModuleId, WrappedModule>,
}

impl VM {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn deploy(&mut self, bytecode: &[u8]) -> Result<ModuleId, Error> {
        let store = wasmer::Store::default();
        let id = ModuleId(self.modules.len());
        let module = WrappedModule::new(&store, bytecode)?;
        self.modules.insert(id, module);
        Ok(id)
    }

    pub fn module(&self, id: ModuleId) -> &WrappedModule {
        self.modules.get(&id).expect("Invalid ModuleId")
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
        let mut session = Session::new(self);
        session.query(id, method_name, arg)
    }

    pub fn session(&self) -> Session {
        Session::new(self)
    }

    pub fn session_mut(&mut self) -> SessionMut {
        SessionMut::new(self)
    }
}
