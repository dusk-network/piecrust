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

/// Struct that describes the state of the Initializer contract
pub struct Initializer {
    value: u8,
}

impl Initializer {
    pub fn init(&mut self, value: u8) {
        self.value = value;
    }
}

/// State of the Initializer contract
static mut STATE: Initializer = Initializer { value: 0x50 };

impl Initializer {
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

/// Expose `Initializer::read_value()` to the host
#[no_mangle]
unsafe fn read_value(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |_: ()| STATE.read_value())
}

/// Expose `Initializer::increment()` to the host
#[no_mangle]
unsafe fn increment(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |_: ()| STATE.increment())
}

/// Expose `Initializer::init()` to the host
#[no_mangle]
unsafe fn init(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |arg: u8| STATE.init(arg))
}
