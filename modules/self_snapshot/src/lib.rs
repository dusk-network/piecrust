// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

#![no_std]
#![feature(
    core_intrinsics,
    lang_items,
    alloc_error_handler,
    arbitrary_self_types
)]

use dallo::{HostAlloc, ModuleId, State};
#[global_allocator]
static ALLOCATOR: HostAlloc = HostAlloc;

const ARGBUF_LEN: usize = 64;

#[no_mangle]
static mut A: [u64; ARGBUF_LEN / 8] = [0; ARGBUF_LEN / 8];
#[no_mangle]
static AL: i32 = ARGBUF_LEN as i32;

#[no_mangle]
static SELF_ID: ModuleId = ModuleId::uninitialized();

static mut STATE: State<SelfSnapshot> =
    unsafe { State::new(SelfSnapshot { crossover: 7 }, &mut A) };

pub struct SelfSnapshot {
    crossover: i32,
}

impl SelfSnapshot {
    pub fn crossover(&self) -> i32 {
        self.crossover
    }

    pub fn set_crossover(&mut self, to: i32) -> i32 {
        let old_val = self.crossover;
        // dallo::debug!(
        //     "setting crossover from {:?} to {:?}",
        //     self.crossover,
        //     to
        // );
        self.crossover = to;
        old_val
    }

    // updates crossover and returns the old value
    pub fn self_call_test_a(self: &mut State<Self>, update: i32) -> i32 {
        let old_value = self.crossover;
        let callee = dallo::self_id();
        let _old: i32 = self.transact(callee, "set_crossover", update);
        assert_eq!(self.crossover, update);
        old_value
    }

    // updates crossover and returns the old value
    pub fn self_call_test_b(&mut self) -> i32 {
        self.set_crossover(self.crossover * 2);
        self.crossover
    }

    pub fn update_and_panic(self: &mut State<Self>, new_value: i32) {
        let old_value = self.crossover;
        let callee = dallo::self_id();

        // What should self.crossover be in this case?

        // A: we live with inconsistencies and communicate them.
        // B: we update self, which then should be passed to the transaction

        if self.query::<_, i32>(callee, "crossover", new_value) == old_value {
            panic!("OH NOES")
        }
    }
}

#[no_mangle]
unsafe fn crossover(a: i32) -> i32 {
    dallo::wrap_query(STATE.buffer(), a, |_: ()| STATE.crossover())
}

#[no_mangle]
unsafe fn set_crossover(a: i32) -> i32 {
    dallo::wrap_transaction(STATE.buffer(), a, |arg: i32| {
        STATE.set_crossover(arg)
    })
}

#[no_mangle]
unsafe fn self_call_test_a(a: i32) -> i32 {
    dallo::wrap_transaction(STATE.buffer(), a, |arg: i32| {
        STATE.self_call_test_a(arg)
    })
}

#[no_mangle]
unsafe fn self_call_test_b(a: i32) -> i32 {
    dallo::wrap_transaction(STATE.buffer(), a, |_: ()| STATE.self_call_test_b())
}
