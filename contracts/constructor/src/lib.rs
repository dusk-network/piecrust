// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Contract that provides and example use of the constructor.

#![no_std]

use piecrust_macros::contract;

/// Struct that describes the state of the Constructor contract
pub struct Constructor {
    value: u8,
}

#[contract]
impl Constructor {
    pub fn init(&mut self, value: u8) {
        self.value = value;
    }
}

/// State of the Constructor contract
static mut STATE: Constructor = Constructor { value: 0x50 };

#[contract]
impl Constructor {
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
