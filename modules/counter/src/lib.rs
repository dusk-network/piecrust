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

use dallo::{ModuleId, State};

#[no_mangle]
static SELF_ID: ModuleId = ModuleId::uninitialized();

static mut STATE: State<Counter> = State::new(Counter { value: 0xfc });

impl Counter {
    pub fn read_value(&self) -> i64 {
        self.value
    }

    pub fn increment(&mut self) {
        let value = self.value + 1;

        // TODO, use dallo::emit() when available
        dallo::emit(value);
        self.value = value;
    }

    pub fn mogrify(&mut self, x: i64) -> i64 {
        let old = self.read_value();
        self.value -= x;
        old
    }
}

#[no_mangle]
unsafe fn read_value(arg_len: u32) -> u32 {
    dallo::wrap_query(arg_len, |_: ()| STATE.read_value())
}

#[no_mangle]
unsafe fn increment(arg_len: u32) -> u32 {
    dallo::wrap_transaction(arg_len, |_: ()| STATE.increment())
}

#[no_mangle]
unsafe fn mogrify(arg_len: u32) -> u32 {
    dallo::wrap_transaction(arg_len, |by| STATE.mogrify(by))
}
