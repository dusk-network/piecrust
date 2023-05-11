// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Module to call another module.

#![feature(arbitrary_self_types)]
#![no_std]

use piecrust_uplink as uplink;
use uplink::{wrap_call, ModuleError, ModuleId, RawCall, RawResult, State};

/// Struct that describes the state of the Callcenter module
pub struct Callcenter;

/// State of the Callcenter module
static mut STATE: State<Callcenter> = State::new(Callcenter);

impl Callcenter {
    /// Read the value of the counter
    pub fn query_counter(&self, counter_id: ModuleId) -> i64 {
        uplink::call(counter_id, "read_value", &()).unwrap()
    }

    /// Increment the counter
    pub fn increment_counter(self: &mut State<Self>, counter_id: ModuleId) {
        uplink::call(counter_id, "increment", &()).unwrap()
    }

    /// Query a module specified by its ID
    pub fn delegate_query(
        &self,
        module_id: ModuleId,
        raw: RawCall,
    ) -> Result<RawResult, ModuleError> {
        uplink::debug!("raw query {:?} at {:?}", raw, module_id);
        uplink::call_raw(module_id, &raw)
    }

    /// Pass the current query
    pub fn query_passthrough(&mut self, raw: RawCall) -> RawCall {
        uplink::debug!("q passthrough {:?}", raw);
        raw
    }

    /// Execute a module specified by its ID
    pub fn delegate_transaction(
        self: &mut State<Self>,
        module_id: ModuleId,
        raw: RawCall,
    ) -> RawResult {
        uplink::call_raw(module_id, &raw).unwrap()
    }

    /// Check whether the current caller is the module itself
    pub fn calling_self(&self, id: ModuleId) -> bool {
        uplink::self_id() == id
    }

    /// Return this module's ID
    pub fn return_self_id(&self) -> ModuleId {
        uplink::self_id()
    }

    /// Return the caller of this module
    pub fn return_caller(&self) -> ModuleId {
        uplink::caller()
    }

    /// Make sure that the caller of this module is the module itself
    pub fn call_self(&self) -> Result<bool, ModuleError> {
        let self_id = uplink::self_id();
        let caller = uplink::caller();

        match caller.is_uninitialized() {
            true => uplink::call(self_id, "call_self", &())
                .expect("querying self should succeed"),
            false => Ok(caller == self_id),
        }
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
    wrap_call(arg_len, |(mod_id, rq): (ModuleId, RawCall)| {
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
    wrap_call(arg_len, |(mod_id, rt): (ModuleId, RawCall)| {
        STATE.delegate_transaction(mod_id, rt)
    })
}
