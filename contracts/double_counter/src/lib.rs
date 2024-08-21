// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Contract to implement a simple double counter that can be read and
//! have either value incremented by one count.

#![no_std]

use piecrust_uplink as uplink;
use uplink::{ContractError, ContractId};

/// Struct that describes the state of the DoubleCounter contract
pub struct DoubleCounter {
    left_value: i64,
    right_value: i64,
}

/// State of the DoubleCounter contract
static mut STATE: DoubleCounter = DoubleCounter {
    left_value: 0xfc,
    right_value: 0xcf,
};

impl DoubleCounter {
    /// Read the value of the counter
    pub fn read_values(&self) -> (i64, i64) {
        (self.left_value, self.right_value)
    }

    /// Increment the value of the left counter by 1
    pub fn increment_left(&mut self) {
        let value = self.left_value + 1;
        self.left_value = value;
    }

    /// Increment the value of the right counter by 1
    pub fn increment_right(&mut self) {
        let value = self.right_value + 1;
        self.right_value = value;
    }

    /// Increment the counter by 1 and call the given contract, with the given
    /// arguments.
    ///
    /// This is intended to test the behavior of the contract on calling a
    /// contract that doesn't exist.
    pub fn increment_left_and_call(
        &mut self,
        contract: ContractId,
    ) -> Result<(), ContractError> {
        let value = self.left_value + 1;
        self.left_value = value;
        uplink::call(contract, "hello", &())
    }
}

/// Expose `Counter::read_value()` to the host
#[no_mangle]
unsafe fn read_values(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |_: ()| STATE.read_values())
}

/// Expose `Counter::increment_left()` to the host
#[no_mangle]
unsafe fn increment_left(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |_: ()| STATE.increment_left())
}

/// Expose `Counter::increment_right()` to the host
#[no_mangle]
unsafe fn increment_right(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |_: ()| STATE.increment_right())
}

/// Expose `Counter::increment_and_call()` to the host
#[no_mangle]
unsafe fn increment_left_and_call(arg_len: u32) -> u32 {
    uplink::wrap_call_unchecked(arg_len, |arg| {
        STATE.increment_left_and_call(arg)
    })
}
