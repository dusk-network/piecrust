// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Contract to emit an event with a given number.

#![no_std]

extern crate alloc;

use alloc::vec::Vec;

use piecrust_uplink as uplink;

/// Struct that describes the state of the eventer contract
pub struct Eventer {
    value: u32,
}

/// State of the eventer contract
static mut STATE: Eventer = Eventer { value: 0 };

impl Eventer {
    /// Emits an event with the given number
    pub fn emit_num(&mut self, num: u32) {
        for i in 0..num {
            uplink::emit("number", i);
        }
    }

    /// Emits an event with the given number, using `emit_raw`
    pub fn emit_num_raw(&mut self, num: u32) {
        for i in 0..num {
            uplink::emit_raw("number", i.to_le_bytes());
        }
    }

    pub fn emit_input(&mut self, input: Vec<u8>) -> (u64, u64) {
        let spent_before = uplink::spent();
        uplink::emit("input", input);
        let spent_after = uplink::spent();
        (spent_before, spent_after)
    }

    pub fn emit_and_mutate(&mut self, value: u32) {
        uplink::emit("number", value);
        self.value = value;
    }

    pub fn read_value(&self) -> u32 {
        self.value
    }
}

/// Expose `Eventer::emit_num()` to the host
#[unsafe(no_mangle)]
unsafe fn emit_events(arg_len: u32) -> u32 {
    // SAFETY: WASM smart contracts are single-threaded, so accessing mutable
    // static via raw pointer is safe - there's no risk of data races.
    unsafe {
        uplink::wrap_call(arg_len, |num| (*&raw mut STATE).emit_num(num))
    }
}

/// Expose `Eventer::emit_num_raw()` to the host
#[unsafe(no_mangle)]
unsafe fn emit_events_raw(arg_len: u32) -> u32 {
    unsafe {
        uplink::wrap_call(arg_len, |num| (*&raw mut STATE).emit_num_raw(num))
    }
}

/// Expose `Eventer::emit_input()` to the host
#[unsafe(no_mangle)]
unsafe fn emit_input(arg_len: u32) -> u32 {
    unsafe {
        uplink::wrap_call(arg_len, |input| (*&raw mut STATE).emit_input(input))
    }
}

/// Expose `Eventer::emit_and_mutate()` to the host
#[unsafe(no_mangle)]
unsafe fn emit_and_mutate(arg_len: u32) -> u32 {
    unsafe {
        uplink::wrap_call(arg_len, |value| {
            (*&raw mut STATE).emit_and_mutate(value)
        })
    }
}

/// Expose `Eventer::read_value()` to the host
#[unsafe(no_mangle)]
unsafe fn read_value(arg_len: u32) -> u32 {
    unsafe {
        uplink::wrap_call(arg_len, |_: ()| (*&raw const STATE).read_value())
    }
}
