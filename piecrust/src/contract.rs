// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::sync::Arc;

use bytecheck::CheckBytes;
use dusk_wasmtime::{Engine, Module};
use piecrust_uplink::ContractId;
use rkyv::{Archive, Deserialize, Serialize};

use crate::error::Error;

pub struct ContractData<'a, A> {
    pub(crate) contract_id: Option<ContractId>,
    pub(crate) constructor_arg: Option<&'a A>,
    pub(crate) owner: Vec<u8>,
}

// `()` is done on purpose, since by default it should be that the constructor
// takes no argument.
impl<'a> ContractData<'a, ()> {
    /// Build a deploy data structure.
    ///
    /// This function returns a builder that can be used to set optional fields
    /// in contract deployment.
    pub fn builder(owner: impl Into<Vec<u8>>) -> ContractDataBuilder<'a, ()> {
        ContractDataBuilder {
            contract_id: None,
            constructor_arg: None,
            owner: owner.into(),
        }
    }
}

impl<'a, A> From<ContractDataBuilder<'a, A>> for ContractData<'a, A> {
    fn from(builder: ContractDataBuilder<'a, A>) -> Self {
        builder.build()
    }
}

pub struct ContractDataBuilder<'a, A> {
    contract_id: Option<ContractId>,
    owner: Vec<u8>,
    constructor_arg: Option<&'a A>,
}

impl<'a, A> ContractDataBuilder<'a, A> {
    /// Set the deployment contract ID.
    pub fn contract_id(mut self, id: ContractId) -> Self {
        self.contract_id = Some(id);
        self
    }

    /// Set the constructor argument for deployment.
    pub fn constructor_arg<B>(self, arg: &B) -> ContractDataBuilder<B> {
        ContractDataBuilder {
            contract_id: self.contract_id,
            owner: self.owner,
            constructor_arg: Some(arg),
        }
    }

    pub fn build(self) -> ContractData<'a, A> {
        ContractData {
            contract_id: self.contract_id,
            constructor_arg: self.constructor_arg,
            owner: self.owner,
        }
    }
}

#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
#[archive_attr(derive(CheckBytes))]
pub struct ContractMetadata {
    pub contract_id: ContractId,
    pub owner: Vec<u8>,
}

#[derive(Clone)]
pub struct WrappedContract {
    serialized: Arc<Vec<u8>>,
}

impl WrappedContract {
    pub fn new<B: AsRef<[u8]>, C: AsRef<[u8]>>(
        engine: &Engine,
        bytecode: B,
        module: Option<C>,
    ) -> Result<Self, Error> {
        let serialized = match module {
            Some(obj) => obj.as_ref().to_vec(),
            _ => {
                let contract = Module::new(engine, bytecode.as_ref())?;
                contract.serialize()?.to_vec()
            }
        };

        Ok(WrappedContract {
            serialized: Arc::new(serialized),
        })
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.serialized
    }
}
