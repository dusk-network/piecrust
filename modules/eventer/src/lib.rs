// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Module to emit an event with a given number.

#![feature(arbitrary_self_types)]
#![no_std]

use piecrust_uplink as uplink;
use uplink::State;

/// Struct that describes the state of the eventer module
pub struct Eventer;

/// State of the eventer module
static mut STATE: State<Eventer> = State::new(Eventer);

impl Eventer {
    /// Emits an event with the given number
    pub fn emit_num(self: &mut State<Eventer>, num: u32) {
        for i in 0..num {
            self.emit(i);
        }
    }
}

/// Expose `Eventer::emit_num()` to the host
#[no_mangle]
unsafe fn emit_events(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |num| STATE.emit_num(num))
}
