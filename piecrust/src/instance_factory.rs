// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use crate::contract::contract_instance::ContractInstance;
use crate::contract::WrappedContract;
use crate::instance::WrappedInstance;
use crate::store::Memory;
use crate::Error;
use crate::Session;
use piecrust_uplink::ContractId;

pub trait InstanceFactory: Send + Sync {
    fn create_instance(
        &self,
        session: Session,
        contract_id: ContractId,
        contract: &WrappedContract,
        memory: Memory,
    ) -> Result<Box<dyn ContractInstance>, Error>;
}

pub struct WasmtimeInstanceFactory;

impl InstanceFactory for WasmtimeInstanceFactory {
    fn create_instance(
        &self,
        session: Session,
        contract_id: ContractId,
        contract: &WrappedContract,
        memory: Memory,
    ) -> Result<Box<dyn ContractInstance>, Error> {
        WrappedInstance::new(session, contract_id, contract, memory)
            .map(|i| Box::new(i) as Box<dyn ContractInstance>)
    }
}
