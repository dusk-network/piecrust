// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Module of a counter that can panic if wanted.

#![feature(arbitrary_self_types)]
#![no_std]

use piecrust_uplink as uplink;
use uplink::State;

/// Struct that describes the state of the fallible counter module
pub struct FallibleCounter {
    value: i64,
}

/// State of the fallible counter module
static mut STATE: State<FallibleCounter> =
    State::new(FallibleCounter { value: 0xfc });

impl FallibleCounter {
    /// Read the value of the counter
    pub fn read_value(&self) -> i64 {
        self.value
    }

    /// Increment the value of the counter and panic if wanted
    pub fn increment(&mut self, panic: bool) {
        let value = self.value + 1;
        self.value = value;
        if panic {
            panic!()
        }
    }
}

/// Expose `FallibleCounter::read_value()` to the host
#[no_mangle]
unsafe fn read_value(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |_: ()| STATE.read_value())
}

/// Expose `FallibleCounter::increment()` to the host
#[no_mangle]
unsafe fn increment(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |panic: bool| STATE.increment(panic))
}
