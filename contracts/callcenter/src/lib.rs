// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Contract to call another contract.

#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use piecrust_uplink as uplink;
use piecrust_uplink::call_with_limit;
use uplink::{wrap_call, ContractError, ContractId};

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

    /// Call specified contract's init method with an empty argument
    pub fn call_init(&mut self, contract_id: ContractId) {
        uplink::call(contract_id, "init", &()).unwrap()
    }

    /// Query a contract specified by its ID
    pub fn delegate_query(
        &self,
        contract_id: ContractId,
        fn_name: String,
        fn_arg: Vec<u8>,
    ) -> Result<Vec<u8>, ContractError> {
        uplink::debug!("raw query {fn_name} at {contract_id:?}");
        uplink::call_raw(contract_id, &fn_name, &fn_arg)
    }

    /// Pass the current query
    pub fn query_passthrough(
        &mut self,
        fn_name: String,
        fn_arg: Vec<u8>,
    ) -> (String, Vec<u8>) {
        uplink::debug!("q passthrough {fn_name}");
        (fn_name, fn_arg)
    }

    /// Execute a contract specified by its ID
    pub fn delegate_transaction(
        &mut self,
        contract_id: ContractId,
        fn_name: String,
        fn_arg: Vec<u8>,
    ) -> Vec<u8> {
        uplink::call_raw(contract_id, &fn_name, &fn_arg).unwrap()
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
    pub fn return_caller(&self) -> Option<ContractId> {
        uplink::caller()
    }

    /// Return the entire call stack of this contract
    pub fn return_callstack(&self) -> Vec<ContractId> {
        uplink::callstack()
    }

    /// Make sure that the caller of this contract is the contract itself
    pub fn call_self(&self) -> Result<bool, ContractError> {
        let self_id = uplink::self_id();
        match uplink::caller() {
            None => uplink::call(self_id, "call_self", &())
                .expect("querying self should succeed"),
            Some(caller) => Ok(caller == self_id),
        }
    }

    /// Return a call stack after calling itself n times
    pub fn call_self_n_times(&self, n: u32) -> Vec<ContractId> {
        let self_id = uplink::self_id();
        match n {
            0 => uplink::callstack(),
            _ => uplink::call(self_id, "call_self_n_times", &(n - 1))
                .expect("calling self should succeed"),
        }
    }

    /// Calls the `spend` function of the `contract` with no arguments, and the
    /// given `gas_limit`, assuming the called function returns `()`. It will
    /// then return the call's result itself.
    pub fn call_spend_with_limit(
        &self,
        contract: ContractId,
        gas_limit: u64,
    ) -> Result<(), ContractError> {
        let res = call_with_limit(contract, "spend", &(), gas_limit);
        uplink::debug!("spend call: {res:?}");
        res
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

/// Expose `Callcenter::call_init()` to the host
#[no_mangle]
unsafe fn call_init(arg_len: u32) -> u32 {
    wrap_call(arg_len, |contract_id| {
        STATE.call_init(contract_id)
    })
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

/// Expose `Callcenter::call_self_n_times()` to the host
#[no_mangle]
unsafe fn call_self_n_times(arg_len: u32) -> u32 {
    wrap_call(arg_len, |n: u32| STATE.call_self_n_times(n))
}

/// Expose `Callcenter::call_spend_with_limit` to the host
#[no_mangle]
unsafe fn call_spend_with_limit(arg_len: u32) -> u32 {
    wrap_call(arg_len, |(contract, gas_limit)| {
        STATE.call_spend_with_limit(contract, gas_limit)
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

/// Expose `Callcenter::return_callstack()` to the host
#[no_mangle]
unsafe fn return_callstack(arg_len: u32) -> u32 {
    wrap_call(arg_len, |_: ()| STATE.return_callstack())
}

/// Expose `Callcenter::delegate_query()` to the host
#[no_mangle]
unsafe fn delegate_query(arg_len: u32) -> u32 {
    wrap_call(arg_len, |(mod_id, fn_name, fn_arg)| {
        STATE.delegate_query(mod_id, fn_name, fn_arg)
    })
}

/// Expose `Callcenter::query_passthrough()` to the host
#[no_mangle]
unsafe fn query_passthrough(arg_len: u32) -> u32 {
    wrap_call(arg_len, |(fn_name, fn_arg)| {
        STATE.query_passthrough(fn_name, fn_arg)
    })
}

/// Expose `Callcenter::delegate_transaction()` to the host
#[no_mangle]
unsafe fn delegate_transaction(arg_len: u32) -> u32 {
    wrap_call(arg_len, |(mod_id, fn_name, fn_arg)| {
        STATE.delegate_transaction(mod_id, fn_name, fn_arg)
    })
}

/// Expose `Callcenter::panik()` to the host
#[no_mangle]
unsafe fn panik(arg_len: u32) -> u32 {
    wrap_call(arg_len, |()| STATE.panik())
}
