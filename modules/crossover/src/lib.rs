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
    value: i32,
}

/// Module ID, initialized by the host when the module is deployed
#[no_mangle]
static SELF_ID: ModuleId = ModuleId::uninitialized();

const INITIAL_VALUE: i32 = 0;

/// State of the crossover module
static mut STATE: State<Crossover> = State::new(Crossover {
    value: INITIAL_VALUE,
});

impl Crossover {
    // Calls the [`set_back_and_panic`] method of the contract `module`,
    // which is assumed to be another Crossover contract.
    //
    // The `module` contract will set their `value` to `value_to_set_forward`,
    // set this contract's `value` to `value_to_set_back`, and then panic.
    //
    // It asserts both this contract and `module` have a `value` set to
    // `INITIAL_VALUE` (i.e., the changes made by the panicking `module` were
    // reverted).
    //
    // Before returning, the contract's `value` is set to `value_to_set`.
    pub fn check_consistent_state_on_errors(
        self: &mut State<Self>,
        module: ModuleId,
        value_to_set: i32,
        value_to_set_forward: i32,
        value_to_set_back: i32,
    ) {
        uplink::debug!("calling panicking module {module:?}");
        self.transact::<_, ()>(
            module,
            "set_back_and_panic",
            &(value_to_set_forward, value_to_set_back),
        )
        .expect_err("should give an error on a panic");

        assert_eq!(
            self.value, INITIAL_VALUE,
            "Our value should not be set due to the panicked call"
        );

        uplink::debug!("querying module {module:?} for their state");
        let other_crossover =
            uplink::query::<_, i32>(module, "crossover", &()).unwrap();

        assert_eq!(
            other_crossover, INITIAL_VALUE,
            "The other contract's value should also not be set due to their panic"
        );

        self.set_crossover(value_to_set);
    }

    // Sets the contract's value and then calls its caller's [`set_crossover`]
    // call to set their value. The caller is assumed to be another crossover
    // contract.
    //
    // It then proceeds to !!panic!!
    pub fn set_back_and_panic(
        self: &mut State<Self>,
        value_to_set: i32,
        value_to_set_back: i32,
    ) {
        self.set_crossover(value_to_set);

        let caller = uplink::caller();
        uplink::debug!("calling back {caller:?}");
        self.transact::<_, ()>(caller, "set_crossover", &value_to_set_back)
            .unwrap();

        uplink::debug!("panicking after setting the crossover");
        panic!("OH NOES");
    }

    /// Return crossover value
    pub fn crossover(&self) -> i32 {
        uplink::debug!("returning crossover: {}", self.value);
        self.value
    }

    /// Update crossover and return old value
    pub fn set_crossover(&mut self, to: i32) -> i32 {
        let old_val = self.value;
        uplink::debug!("setting crossover from {old_val} to {to}");
        self.value = to;
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

/// Expose `Crossover::check_consistent_state_on_errors()` to the host
#[no_mangle]
unsafe fn check_consistent_state_on_errors(arg_len: u32) -> u32 {
    uplink::wrap_transaction(arg_len, |(module, s, sf, sb)| {
        STATE.check_consistent_state_on_errors(module, s, sf, sb)
    })
}

/// Expose `Crossover::set_back_and_panic()` to the host
#[no_mangle]
unsafe fn set_back_and_panic(arg_len: u32) -> u32 {
    uplink::wrap_transaction(arg_len, |(v, vb)| STATE.set_back_and_panic(v, vb))
}
