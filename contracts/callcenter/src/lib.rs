// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Contract to call another contract.

#![no_std]

use piecrust_uplink as uplink;
use piecrust_uplink::call_with_limit;
use uplink::{wrap_call, ContractError, ContractId, RawCall, RawResult};

/// Struct that describes the state of the Callcenter contract
pub struct Callcenter;

/// State of the Callcenter contract
static mut STATE: Callcenter = Callcenter;

impl Callcenter {
    /// Read the value of the counter
    pub fn query_counter(&self, counter_id: ContractId) -> i64 {
        uplink::call(counter_id, "read_value", &()).unwrap()
    }

    /// Increment the counter
    pub fn increment_counter(&mut self, counter_id: ContractId) {
        uplink::call(counter_id, "increment", &()).unwrap()
    }

    /// Query a contract specified by its ID
    pub fn delegate_query(
        &self,
        contract_id: ContractId,
        raw: RawCall,
    ) -> Result<RawResult, ContractError> {
        uplink::debug!("raw query {:?} at {:?}", raw, contract_id);
        uplink::call_raw(contract_id, &raw)
    }

    /// Pass the current query
    pub fn query_passthrough(&mut self, raw: RawCall) -> RawCall {
        uplink::debug!("q passthrough {:?}", raw);
        raw
    }

    /// Execute a contract specified by its ID
    pub fn delegate_transaction(
        &mut self,
        contract_id: ContractId,
        raw: RawCall,
    ) -> RawResult {
        uplink::call_raw(contract_id, &raw).unwrap()
    }

    /// Check whether the current caller is the contract itself
    pub fn calling_self(&self, id: ContractId) -> bool {
        uplink::self_id() == id
    }

    /// Return this contract's ID
    pub fn return_self_id(&self) -> ContractId {
        uplink::self_id()
    }

    /// Return the caller of this contract
    pub fn return_caller(&self) -> ContractId {
        uplink::caller()
    }

    /// Make sure that the caller of this contract is the contract itself
    pub fn call_self(&self) -> Result<bool, ContractError> {
        let self_id = uplink::self_id();
        let caller = uplink::caller();

        match caller.is_uninitialized() {
            true => uplink::call(self_id, "call_self", &())
                .expect("querying self should succeed"),
            false => Ok(caller == self_id),
        }
    }

    /// Calls the `spend` function of the `contract` with no arguments, and the
    /// given `points_limit`, assuming the called function returns `()`. It will
    /// then return the call's result itself.
    pub fn call_spend_with_limit(
        &self,
        contract: ContractId,
        points_limit: u64,
    ) -> Result<(), ContractError> {
        call_with_limit(contract, "spend", &(), points_limit)?;
        Ok(())
    }

    /// Just panic.
    pub fn panik(&self) {
        panic!("panik");
    }
}

/// Expose `Callcenter::query_counter()` to the host
#[no_mangle]
unsafe fn query_counter(arg_len: u32) -> u32 {
    wrap_call(arg_len, |counter_id| STATE.query_counter(counter_id))
}

/// Expose `Callcenter::increment_counter()` to the host
#[no_mangle]
unsafe fn increment_counter(arg_len: u32) -> u32 {
    wrap_call(arg_len, |counter_id| STATE.increment_counter(counter_id))
}

/// Expose `Callcenter::calling_self()` to the host
#[no_mangle]
unsafe fn calling_self(arg_len: u32) -> u32 {
    wrap_call(arg_len, |self_id| STATE.calling_self(self_id))
}

/// Expose `Callcenter::call_self()` to the host
#[no_mangle]
unsafe fn call_self(arg_len: u32) -> u32 {
    wrap_call(arg_len, |_: ()| STATE.call_self())
}

/// Expose `Callcenter::call_spend_with_limit` to the host
#[no_mangle]
unsafe fn call_spend_with_limit(arg_len: u32) -> u32 {
    wrap_call(arg_len, |(contract, points_limit)| {
        STATE.call_spend_with_limit(contract, points_limit)
    })
}

/// Expose `Callcenter::return_self_id()` to the host
#[no_mangle]
unsafe fn return_self_id(arg_len: u32) -> u32 {
    wrap_call(arg_len, |_: ()| STATE.return_self_id())
}

/// Expose `Callcenter::return_caller()` to the host
#[no_mangle]
unsafe fn return_caller(arg_len: u32) -> u32 {
    wrap_call(arg_len, |_: ()| STATE.return_caller())
}

/// Expose `Callcenter::delegate_query()` to the host
#[no_mangle]
unsafe fn delegate_query(arg_len: u32) -> u32 {
    wrap_call(arg_len, |(mod_id, rq): (ContractId, RawCall)| {
        STATE.delegate_query(mod_id, rq)
    })
}

/// Expose `Callcenter::query_passthrough()` to the host
#[no_mangle]
unsafe fn query_passthrough(arg_len: u32) -> u32 {
    wrap_call(arg_len, |rq: RawCall| STATE.query_passthrough(rq))
}

/// Expose `Callcenter::delegate_transaction()` to the host
#[no_mangle]
unsafe fn delegate_transaction(arg_len: u32) -> u32 {
    wrap_call(arg_len, |(mod_id, rt): (ContractId, RawCall)| {
        STATE.delegate_transaction(mod_id, rt)
    })
}

/// Expose `Callcenter::panik()` to the host
#[no_mangle]
unsafe fn panik(arg_len: u32) -> u32 {
    wrap_call(arg_len, |()| STATE.panik())
}
