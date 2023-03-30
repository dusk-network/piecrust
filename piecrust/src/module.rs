// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::sync::Arc;
use wasmer::Module;

use bytecheck::CheckBytes;
use rkyv::{Archive, Deserialize, Serialize};

use crate::error::Error;
use crate::instance::Store;
use piecrust_uplink::ModuleId;

pub struct DeployData<'a, A> {
    pub(crate) module_id: Option<ModuleId>,
    pub(crate) constructor_arg: Option<&'a A>,
    pub(crate) owner: [u8; 32],
}

// `()` is done on purpose, since by default it should be that the constructor
// takes no argument.
impl<'a> DeployData<'a, ()> {
    /// Build a deploy data structure.
    ///
    /// This function returns a builder that can be used to set optional fields
    /// in module deployment.
    pub fn builder(owner: [u8; 32]) -> DeployDataBuilder<'a, ()> {
        DeployDataBuilder {
            module_id: None,
            constructor_arg: None,
            owner,
        }
    }
}

impl<'a, A> From<DeployDataBuilder<'a, A>> for DeployData<'a, A> {
    fn from(builder: DeployDataBuilder<'a, A>) -> Self {
        builder.build()
    }
}

pub struct DeployDataBuilder<'a, A> {
    module_id: Option<ModuleId>,
    owner: [u8; 32],
    constructor_arg: Option<&'a A>,
}

impl<'a, A> DeployDataBuilder<'a, A> {
    /// Set the deployment module ID.
    pub fn module_id(mut self, id: ModuleId) -> Self {
        self.module_id = Some(id);
        self
    }

    /// Set the constructor argument for deployment.
    pub fn constructor_arg<B>(self, arg: &B) -> DeployDataBuilder<B> {
        DeployDataBuilder {
            module_id: self.module_id,
            owner: self.owner,
            constructor_arg: Some(arg),
        }
    }

    pub fn build(self) -> DeployData<'a, A> {
        DeployData {
            module_id: self.module_id,
            constructor_arg: self.constructor_arg,
            owner: self.owner,
        }
    }
}

#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
#[archive_attr(derive(CheckBytes))]
pub struct ModuleMetadata {
    pub module_id: ModuleId,
    pub owner: [u8; 32],
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
