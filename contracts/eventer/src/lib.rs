// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Contract to emit an event with a given number.

#![no_std]

use piecrust_macros::contract;
use piecrust_uplink as uplink;

/// Struct that describes the state of the eventer contract
pub struct Eventer;

/// State of the eventer contract
static mut STATE: Eventer = Eventer;

#[contract]
impl Eventer {
    /// Emits the given number of events
    pub fn emit_events(&mut self, num: u32) {
        for i in 0..num {
            uplink::emit("number", i);
        }
    }
}
