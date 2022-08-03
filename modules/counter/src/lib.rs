// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

#![feature(arbitrary_self_types)]
#![no_std]
#![no_main]

#[global_allocator]
static ALLOCATOR: dallo::HostAlloc = dallo::HostAlloc;

#[derive(Default)]
pub struct Counter {
    value: i64,
}

use dallo::{State, MODULE_ID_BYTES};

const ARGBUF_LEN: usize = 64;

#[no_mangle]
static mut A: [u64; ARGBUF_LEN / 8] = [0; ARGBUF_LEN / 8];
#[no_mangle]
static AL: i32 = ARGBUF_LEN as i32;

#[no_mangle]
static SELF_ID: [u8; MODULE_ID_BYTES] = [0u8; MODULE_ID_BYTES];

static mut STATE: State<Counter> =
    unsafe { State::new(Counter { value: 0xfc }, &mut A) };

impl Counter {
    pub fn read_value(self: &State<Counter>) -> i64 {
        self.value
    }

    pub fn increment(self: &mut State<Counter>) {
        self.value += 1;
    }

    pub fn mogrify(self: &mut State<Counter>, x: i64) -> i64 {
        let old = self.read_value();
        self.value -= x;
        old
    }
}

#[no_mangle]
unsafe fn read_value(a: i32) -> i32 {
    dallo::wrap_query(STATE.buffer(), a, |_: ()| STATE.read_value())
}

#[no_mangle]
unsafe fn increment(a: i32) -> i32 {
    dallo::wrap_transaction(STATE.buffer(), a, |_: ()| STATE.increment())
}

#[no_mangle]
unsafe fn mogrify(a: i32) -> i32 {
    dallo::wrap_transaction(STATE.buffer(), a, |by| STATE.mogrify(by))
}
