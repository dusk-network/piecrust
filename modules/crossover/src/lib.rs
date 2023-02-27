// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Module to test the crossover functionality.

#![no_std]
#![feature(core_intrinsics, lang_items, arbitrary_self_types)]

use piecrust_uplink as uplink;
use uplink::{ModuleId, State};

/// Struct that describes the state of the crossover module
pub struct Crossover {
    crossover: i32,
}

/// Module ID, initialized by the host when the module is deployed
#[no_mangle]
static SELF_ID: ModuleId = ModuleId::uninitialized();

const INITIAL_VALUE: i32 = 0;

/// State of the crossover module
static mut STATE: State<Crossover> = State::new(Crossover {
    crossover: INITIAL_VALUE,
});

impl Crossover {
    // Calls another contract - which is assumed to be another crossover
    // contract - with the "set_and_panic" call.
    //
    // The other contract will first set their state, call this contract to set
    // the crossover, and then panic.
    //
    // We then proceed to query the contract, and return true if both contract's
    // states were unchanged. Before returning, the contract's state is set to
    // the new `value`.
    pub fn call_panicking_and_set(
        self: &mut State<Self>,
        module: ModuleId,
        value: i32,
    ) -> bool {
        uplink::debug!("Calling panicking module {module:?}");
        self.transact::<_, ()>(module, "set_call_and_panic", &value)
            .unwrap_err();

        let self_is_initial = self.crossover == INITIAL_VALUE;

        uplink::debug!("Querying module {module:?} for their state");
        let other_crossover =
            uplink::query::<_, i32>(module, "crossover", &value).unwrap();
        let other_is_initial = other_crossover == INITIAL_VALUE;

        self.set_crossover(value);

        self_is_initial && other_is_initial
    }

    // Sets the crossover to a new `value`, calls the calling contract - which
    // is assumed to be another crossover contract - with "set_crossover" and
    // panics afterwards.
    pub fn set_call_and_panic(self: &mut State<Self>, value: i32) {
        self.set_crossover(value);

        let caller = uplink::caller();
        uplink::debug!("calling back {caller:?}");
        self.transact::<_, ()>(caller, "set_crossover", &value)
            .unwrap();

        uplink::debug!("panicking after setting the crossover");
        panic!("OH NOES");
    }

    /// Return crossover value
    pub fn crossover(&self) -> i32 {
        self.crossover
    }

    /// Update crossover and return old value
    pub fn set_crossover(&mut self, to: i32) -> i32 {
        let old_val = self.crossover;
        uplink::debug!("setting crossover from {old_val} to {to}");
        self.crossover = to;
        old_val
    }
}

/// Expose `Crossover::crossover()` to the host
#[no_mangle]
unsafe fn crossover(arg_len: u32) -> u32 {
    uplink::wrap_query(arg_len, |_: ()| STATE.crossover())
}

/// Expose `Crossover::set_crossover()` to the host
#[no_mangle]
unsafe fn set_crossover(arg_len: u32) -> u32 {
    uplink::wrap_transaction(arg_len, |arg: i32| STATE.set_crossover(arg))
}

/// Expose `Crossover::call_panicking_and_set()` to the host
#[no_mangle]
unsafe fn call_panicking_and_set(arg_len: u32) -> u32 {
    uplink::wrap_transaction(arg_len, |(module, value)| {
        STATE.call_panicking_and_set(module, value)
    })
}

/// Expose `Crossover::set_call_and_panic()` to the host
#[no_mangle]
unsafe fn set_call_and_panic(arg_len: u32) -> u32 {
    uplink::wrap_transaction(arg_len, |value| STATE.set_call_and_panic(value))
}
