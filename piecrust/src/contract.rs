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

pub struct ContractData<'a, A, const N: usize> {
    pub(crate) contract_id: Option<ContractId>,
    pub(crate) constructor_arg: Option<&'a A>,
    pub(crate) owner: [u8; N],
}

// `()` is done on purpose, since by default it should be that the constructor
// takes no argument.
impl<'a, const N: usize> ContractData<'a, (), N> {
    /// Build a deploy data structure.
    ///
    /// This function returns a builder that can be used to set optional fields
    /// in contract deployment.
    pub fn builder(owner: [u8; N]) -> ContractDataBuilder<'a, (), N> {
        ContractDataBuilder {
            contract_id: None,
            constructor_arg: None,
            owner,
        }
    }
}

impl<'a, A, const N: usize> From<ContractDataBuilder<'a, A, N>>
    for ContractData<'a, A, N>
{
    fn from(builder: ContractDataBuilder<'a, A, N>) -> Self {
        builder.build()
    }
}

pub struct ContractDataBuilder<'a, A, const N: usize> {
    contract_id: Option<ContractId>,
    owner: [u8; N],
    constructor_arg: Option<&'a A>,
}

impl<'a, A, const N: usize> ContractDataBuilder<'a, A, N> {
    /// Set the deployment contract ID.
    pub fn contract_id(mut self, id: ContractId) -> Self {
        self.contract_id = Some(id);
        self
    }

    /// Set the constructor argument for deployment.
    pub fn constructor_arg<B>(self, arg: &B) -> ContractDataBuilder<B, N> {
        ContractDataBuilder {
            contract_id: self.contract_id,
            owner: self.owner,
            constructor_arg: Some(arg),
        }
    }

    pub fn build(self) -> ContractData<'a, A, N> {
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
        objectcode: Option<C>,
    ) -> Result<Self, Error> {
        let serialized = match objectcode {
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
