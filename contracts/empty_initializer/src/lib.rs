// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Contract that provides an example use of the init method.
//! The init method provides a way to initialize the state of the contract and execute other logic only once at the time of deployment.
//! 
//! The init method can be partially compared to the functionality of a constructor in other languages.

#![no_std]

use piecrust_uplink as uplink;

/// Struct that describes the state of the Init contract
pub struct EmptyInitializer {
    value: u8,
}

impl EmptyInitializer {
    pub fn init(&mut self) {
        self.value = 0x10;
    }
}

/// State of the EmptyInitializer contract
static mut STATE: EmptyInitializer = EmptyInitializer { value: 0x00 };

impl EmptyInitializer {
    /// Read the value of the contract state
    pub fn read_value(&self) -> u8 {
        self.value
    }

    /// Increment the value  by 1
    pub fn increment(&mut self) {
        let value = self.value + 1;
        self.value = value;
    }
}

/// Expose `EmptyInitializer::read_value()` to the host
#[no_mangle]
unsafe fn read_value(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |_: ()| STATE.read_value())
}

/// Expose `EmptyInitializer::increment()` to the host
#[no_mangle]
unsafe fn increment(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |_: ()| STATE.increment())
}

/// Expose `EmptyInitializer::init()` to the host
#[no_mangle]
unsafe fn init(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |()| STATE.init())
}
