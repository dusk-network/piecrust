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

pub struct ModuleData<'a, A, const N: usize> {
    pub(crate) module_id: Option<ModuleId>,
    pub(crate) constructor_arg: Option<&'a A>,
    pub(crate) owner: [u8; N],
}

// `()` is done on purpose, since by default it should be that the constructor
// takes no argument.
impl<'a, const N: usize> ModuleData<'a, (), N> {
    /// Build a deploy data structure.
    ///
    /// This function returns a builder that can be used to set optional fields
    /// in module deployment.
    pub fn builder(owner: [u8; N]) -> ModuleDataBuilder<'a, (), N> {
        ModuleDataBuilder {
            module_id: None,
            constructor_arg: None,
            owner,
        }
    }
}

impl<'a, A, const N: usize> From<ModuleDataBuilder<'a, A, N>>
    for ModuleData<'a, A, N>
{
    fn from(builder: ModuleDataBuilder<'a, A, N>) -> Self {
        builder.build()
    }
}

pub struct ModuleDataBuilder<'a, A, const N: usize> {
    module_id: Option<ModuleId>,
    owner: [u8; N],
    constructor_arg: Option<&'a A>,
}

impl<'a, A, const N: usize> ModuleDataBuilder<'a, A, N> {
    /// Set the deployment module ID.
    pub fn module_id(mut self, id: ModuleId) -> Self {
        self.module_id = Some(id);
        self
    }

    /// Set the constructor argument for deployment.
    pub fn constructor_arg<B>(self, arg: &B) -> ModuleDataBuilder<B, N> {
        ModuleDataBuilder {
            module_id: self.module_id,
            owner: self.owner,
            constructor_arg: Some(arg),
        }
    }

    pub fn build(self) -> ModuleData<'a, A, N> {
        ModuleData {
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
    pub owner: Vec<u8>,
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
