// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Contract that emits through another contract and then panics.

#![no_std]

use piecrust_uplink as uplink;
use uplink::ContractId;

const EVENT_VALUE: u32 = 42;

/// Struct that describes the state of the event reverter contract
pub struct EventReverter;

/// State of the event reverter contract
static mut STATE: EventReverter = EventReverter;

impl EventReverter {
    /// Call the given eventer and panic after the eventer emitted and mutated.
    pub fn emit_then_panic(&self, eventer: ContractId) {
        if uplink::call::<_, ()>(eventer, "emit_and_mutate", &EVENT_VALUE)
            .is_err()
        {
            panic!("eventer call should succeed");
        }

        panic!("event reverter panic");
    }
}

/// Expose `EventReverter::emit_then_panic()` to the host
#[unsafe(no_mangle)]
unsafe fn emit_then_panic(arg_len: u32) -> u32 {
    unsafe {
        uplink::wrap_call(arg_len, |eventer| {
            (*&raw const STATE).emit_then_panic(eventer)
        })
    }
}
