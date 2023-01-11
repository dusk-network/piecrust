// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Module to test the crossover functionality.

#![no_std]
#![feature(core_intrinsics, lang_items, arbitrary_self_types)]

use piecrust_uplink as uplink;
use uplink::{ModuleId, RawTransaction, State};

/// Struct that describes the state of the self snapshot module
pub struct SelfSnapshot {
    crossover: i32,
}

/// Module id, initialized by the host when the module is deployed
#[no_mangle]
static SELF_ID: ModuleId = ModuleId::uninitialized();

/// State of the self snapshot module
static mut STATE: State<SelfSnapshot> =
    State::new(SelfSnapshot { crossover: 7 });

impl SelfSnapshot {
    /// Return crossover value
    pub fn crossover(&self) -> i32 {
        self.crossover
    }

    /// Update crossover and return old value
    pub fn set_crossover(&mut self, to: i32) -> i32 {
        let old_val = self.crossover;
        uplink::debug!(
            "setting crossover from {:?} to {:?}",
            self.crossover,
            to
        );
        self.crossover = to;
        old_val
    }

    /// Test `set_crossover` functionality through a host transaction
    pub fn self_call_test_a(self: &mut State<Self>, update: i32) -> i32 {
        let old_value = self.crossover;
        let callee = uplink::self_id();
        self.transact::<_, i32>(callee, "set_crossover", update)
            .unwrap();

        assert_eq!(self.crossover, update);
        old_value
    }

    /// Test `set_crossover` functionality through a host raw transaction
    pub fn self_call_test_b(
        self: &mut State<Self>,
        target: ModuleId,
        raw_transaction: RawTransaction,
    ) -> i32 {
        let co = self.crossover;
        self.set_crossover(co * 2);
        self.transact_raw(target, raw_transaction).unwrap();
        self.crossover
    }

    /// Update crossover and panic
    pub fn update_and_panic(&mut self, new_value: i32) {
        let old_value = self.crossover;
        let callee = uplink::self_id();

        // What should self.crossover be in this case?

        // A: we live with inconsistencies and communicate them.
        // B: we update self, which then should be passed to the transaction

        let q = uplink::query::<_, i32>(callee, "crossover", new_value);

        match q {
            Ok(old) if old == old_value => panic!("OH NOES"),
            _ => (),
        }
    }
}

/// Expose `SelfSnapshot::crossover()` to the host
#[no_mangle]
unsafe fn crossover(arg_len: u32) -> u32 {
    uplink::wrap_query(arg_len, |_: ()| STATE.crossover())
}

/// Expose `SelfSnapshot::set_crossover()` to the host
#[no_mangle]
unsafe fn set_crossover(arg_len: u32) -> u32 {
    uplink::wrap_transaction(arg_len, |arg: i32| STATE.set_crossover(arg))
}

/// Expose `SelfSnapshot::self_call_test_a()` to the host
#[no_mangle]
unsafe fn self_call_test_a(arg_len: u32) -> u32 {
    uplink::wrap_transaction(arg_len, |arg: i32| STATE.self_call_test_a(arg))
}

/// Expose `SelfSnapshot::self_call_test_b()` to the host
#[no_mangle]
unsafe fn self_call_test_b(arg_len: u32) -> u32 {
    uplink::wrap_transaction(arg_len, |(target, transaction)| {
        STATE.self_call_test_b(target, transaction)
    })
}
