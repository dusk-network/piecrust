// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

#![no_std]
#![feature(core_intrinsics, lang_items, arbitrary_self_types)]

use uplink::{ModuleId, RawTransaction, State};

#[no_mangle]
static SELF_ID: ModuleId = ModuleId::uninitialized();

static mut STATE: State<SelfSnapshot> =
    State::new(SelfSnapshot { crossover: 7 });

pub struct SelfSnapshot {
    crossover: i32,
}

impl SelfSnapshot {
    pub fn crossover(&self) -> i32 {
        self.crossover
    }

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

    // updates crossover and returns the old value
    pub fn self_call_test_a(self: &mut State<Self>, update: i32) -> i32 {
        let old_value = self.crossover;
        let callee = uplink::self_id();
        self.transact::<_, i32>(callee, "set_crossover", update);

        assert_eq!(self.crossover, update);
        old_value
    }

    // updates crossover and returns the old value
    pub fn self_call_test_b(
        self: &mut State<Self>,
        target: ModuleId,
        raw_transaction: RawTransaction,
    ) -> i32 {
        let co = self.crossover;
        self.set_crossover(co * 2);
        self.transact_raw(target, raw_transaction);
        self.crossover
    }

    pub fn update_and_panic(&mut self, new_value: i32) {
        let old_value = self.crossover;
        let callee = uplink::self_id();

        // What should self.crossover be in this case?

        // A: we live with inconsistencies and communicate them.
        // B: we update self, which then should be passed to the transaction

        if uplink::query::<_, i32>(callee, "crossover", new_value) == old_value
        {
            panic!("OH NOES")
        }
    }
}

#[no_mangle]
unsafe fn crossover(arg_len: u32) -> u32 {
    uplink::wrap_query(arg_len, |_: ()| STATE.crossover())
}

#[no_mangle]
unsafe fn set_crossover(arg_len: u32) -> u32 {
    uplink::wrap_transaction(arg_len, |arg: i32| STATE.set_crossover(arg))
}

#[no_mangle]
unsafe fn self_call_test_a(arg_len: u32) -> u32 {
    uplink::wrap_transaction(arg_len, |arg: i32| STATE.self_call_test_a(arg))
}

#[no_mangle]
unsafe fn self_call_test_b(arg_len: u32) -> u32 {
    uplink::wrap_transaction(arg_len, |(target, transaction)| {
        STATE.self_call_test_b(target, transaction)
    })
}
