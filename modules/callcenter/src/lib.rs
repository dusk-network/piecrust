// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

#![feature(arbitrary_self_types)]
#![no_std]
#![no_main]

use piecrust_uplink::{
    wrap_query, wrap_transaction, ModuleId, RawQuery, RawResult,
    RawTransaction, State,
};

#[derive(Default)]
pub struct Callcenter;

#[no_mangle]
static SELF_ID: ModuleId = ModuleId::uninitialized();

static mut STATE: State<Callcenter> = State::new(Callcenter);

impl Callcenter {
    pub fn query_counter(&self, counter_id: ModuleId) -> i64 {
        let result = piecrust_uplink::query(counter_id, "read_value", ());
        result
    }

    pub fn increment_counter(self: &mut State<Self>, counter_id: ModuleId) {
        self.transact(counter_id, "increment", ())
    }

    pub fn delegate_query(
        &self,
        module_id: ModuleId,
        raw: RawQuery,
    ) -> RawResult {
        piecrust_uplink::debug!("raw query {:?} at {:?}", raw, module_id);
        piecrust_uplink::query_raw(module_id, raw)
    }

    pub fn query_passthrough(&mut self, raw: RawQuery) -> RawQuery {
        piecrust_uplink::debug!("q passthrough {:?}", raw);
        raw
    }

    pub fn delegate_transaction(
        self: &mut State<Self>,
        module_id: ModuleId,
        raw: RawTransaction,
    ) -> RawResult {
        self.transact_raw(module_id, raw)
    }

    pub fn calling_self(&self, id: ModuleId) -> bool {
        piecrust_uplink::self_id() == id
    }

    pub fn return_self_id(&self) -> ModuleId {
        piecrust_uplink::self_id()
    }

    pub fn return_caller(&self) -> ModuleId {
        piecrust_uplink::caller()
    }

    pub fn call_self(&self) -> bool {
        let self_id = piecrust_uplink::self_id();
        let caller = piecrust_uplink::caller();

        match caller.is_uninitialized() {
            true => piecrust_uplink::query(self_id, "call_self", ()),
            false => caller == self_id,
        }
    }
}

#[no_mangle]
unsafe fn query_counter(arg_len: u32) -> u32 {
    wrap_query(arg_len, |counter_id| STATE.query_counter(counter_id))
}

#[no_mangle]
unsafe fn increment_counter(arg_len: u32) -> u32 {
    wrap_transaction(arg_len, |counter_id| STATE.increment_counter(counter_id))
}

#[no_mangle]
unsafe fn calling_self(arg_len: u32) -> u32 {
    wrap_query(arg_len, |self_id| STATE.calling_self(self_id))
}

#[no_mangle]
unsafe fn call_self(arg_len: u32) -> u32 {
    wrap_query(arg_len, |_: ()| STATE.call_self())
}

#[no_mangle]
unsafe fn return_self_id(arg_len: u32) -> u32 {
    wrap_query(arg_len, |_: ()| STATE.return_self_id())
}

#[no_mangle]
unsafe fn return_caller(arg_len: u32) -> u32 {
    wrap_query(arg_len, |_: ()| STATE.return_caller())
}

#[no_mangle]
unsafe fn delegate_query(arg_len: u32) -> u32 {
    wrap_query(arg_len, |(mod_id, rq): (ModuleId, RawQuery)| {
        STATE.delegate_query(mod_id, rq)
    })
}

#[no_mangle]
unsafe fn query_passthrough(arg_len: u32) -> u32 {
    wrap_query(arg_len, |rq: RawQuery| STATE.query_passthrough(rq))
}

#[no_mangle]
unsafe fn delegate_transaction(arg_len: u32) -> u32 {
    wrap_transaction(arg_len, |(mod_id, rt): (ModuleId, RawTransaction)| {
        STATE.delegate_transaction(mod_id, rt)
    })
}
