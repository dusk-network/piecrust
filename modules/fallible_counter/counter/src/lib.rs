// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

#![feature(arbitrary_self_types)]
#![no_std]
#![no_main]

use piecrust_uplink as uplink;
use uplink::{ModuleId, State};

#[derive(Default)]
pub struct Counter {
    value: i64,
}

#[no_mangle]
static SELF_ID: ModuleId = ModuleId::uninitialized();

static mut STATE: State<Counter> = State::new(Counter { value: 0xfc });

impl Counter {
    pub fn read_value(&self) -> i64 {
        self.value
    }

    pub fn increment(&mut self) {
        let value = self.value + 1;
        self.value = value;
    }
}

#[no_mangle]
unsafe fn read_value(arg_len: u32) -> u32 {
    uplink::wrap_query(arg_len, |_: ()| STATE.read_value())
}

#[no_mangle]
unsafe fn increment(arg_len: u32) -> u32 {
    uplink::wrap_transaction(arg_len, |_: ()| STATE.increment())
}
