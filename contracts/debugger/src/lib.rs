// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Simple debug contract.

#![no_std]

extern crate alloc;

use piecrust_macros::contract;
use piecrust_uplink as uplink;

/// Struct that describes the state of the debug contract
pub struct Debug;

/// State of the debug contract
static mut STATE: Debug = Debug;

#[contract]
impl Debug {
    /// Print debug information
    pub fn debug(&self, string: alloc::string::String) {
        uplink::debug!("What a string! {}", string);
    }

    /// Panic execution
    fn panic(&self) {
        panic!("It's never too late to panic");
    }
}
