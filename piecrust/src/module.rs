// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::sync::Arc;
use wasmer::Module;

use crate::error::Error;
use crate::instance::Store;

pub struct DeployData<Arg> {
    pub id: Option<[u8; 32]>,
    pub constructor_arg: Option<Arg>,
    pub owner: [u8; 32],
}

impl<Arg> DeployData<Arg> {
    pub fn new(
        self_id: Option<[u8; 32]>,
        constructor_arg: Option<Arg>,
        owner: [u8; 32],
    ) -> Self {
        Self {
            id: self_id,
            constructor_arg,
            owner,
        }
    }

    pub fn from(owner: [u8; 32]) -> Self {
        Self {
            id: None,
            constructor_arg: None,
            owner,
        }
    }
}

#[derive(Clone)]
pub struct WrappedModule {
    serialized: Arc<Vec<u8>>,
}

impl WrappedModule {
    pub fn new<B: AsRef<[u8]>, C: AsRef<[u8]>>(
        bytecode: B,
        objectcode: Option<C>,
    ) -> Result<Self, Error> {
        let store = Store::new_store();
        let serialized = match objectcode {
            Some(obj) => obj.as_ref().to_vec(),
            _ => {
                let module = Module::new(&store, bytecode.as_ref())?;
                module.serialize()?.to_vec()
            }
        };

        Ok(WrappedModule {
            serialized: Arc::new(serialized),
        })
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.serialized
    }
}
