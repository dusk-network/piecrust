// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Contract to emit an event with a given number.

#![no_std]

use piecrust_uplink as uplink;

/// Struct that describes the state of the eventer contract
pub struct Eventer;

/// State of the eventer contract
static mut STATE: Eventer = Eventer;

impl Eventer {
    /// Emits an event with the given number
    pub fn emit_num(&mut self, num: u32) {
        for i in 0..num {
            uplink::emit("number", i);
        }
    }
}

/// Expose `Eventer::emit_num()` to the host
#[no_mangle]
unsafe fn emit_events(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |num| STATE.emit_num(num))
}
