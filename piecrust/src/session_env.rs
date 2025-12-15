// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use crate::contract::ContractMetadata;
use crate::{CallTreeElem, Error};
use piecrust_uplink::{ContractId, Event};
use std::any::Any;

pub trait SessionEnv: Send + Sync + Any {
    // fn instance(
    //     &self,
    //     contract_id: &ContractId,
    // ) -> Option<WrappedInstance>;
    fn push_event(&mut self, event: Event);
    fn push_feed(&mut self, data: Vec<u8>) -> Result<(), Error>;
    fn nth_from_top(&self, n: usize) -> Option<CallTreeElem>;
    fn push_callstack(
        &mut self,
        contract_id: ContractId,
        limit: u64,
    ) -> Result<CallTreeElem, Error>;
    fn move_up_call_tree(&mut self, spent: u64);
    fn move_up_prune_call_tree(&mut self);
    fn revert_callstack(&mut self) -> Result<(), std::io::Error>;
    fn call_ids(&self) -> Vec<&ContractId>;
    fn meta(&self, name: &str) -> Option<Vec<u8>>;
    fn contract_metadata(
        &mut self,
        contract_id: &ContractId,
    ) -> Option<&ContractMetadata>;
}
