// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Contract that provides and example use of the constructor.

#![no_std]

use piecrust_uplink as uplink;

/// Struct that describes the state of the Constructor contract
pub struct EmptyConstructor {
    value: u8,
}

impl EmptyConstructor {
    pub fn init(&mut self) {
        self.value = 0x10;
    }
}

/// State of the EmptyConstructor contract
static mut STATE: EmptyConstructor = EmptyConstructor { value: 0x00 };

impl EmptyConstructor {
    /// Read the value of the constructor contract state
    pub fn read_value(&self) -> u8 {
        self.value
    }

    /// Increment the value  by 1
    pub fn increment(&mut self) {
        let value = self.value + 1;
        self.value = value;
    }
}

/// Expose `EmptyConstructor::read_value()` to the host
#[no_mangle]
unsafe fn read_value(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |_: ()| STATE.read_value())
}

/// Expose `EmptyConstructor::increment()` to the host
#[no_mangle]
unsafe fn increment(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |_: ()| STATE.increment())
}

/// Expose `EmptyConstructor::init()` to the host
#[no_mangle]
unsafe fn init(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |()| STATE.init())
}
