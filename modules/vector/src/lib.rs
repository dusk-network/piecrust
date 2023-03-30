// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Module that implements the functionalities of a simple vector.

#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use piecrust_uplink as uplink;
use uplink::State;

/// Struct that describes the state of the vector module
pub struct Vector {
    a: Vec<i16>,
}

/// State of the vector module
static mut STATE: State<Vector> = State::new(Vector { a: Vec::new() });

impl Vector {
    /// Push an item to the vector
    pub fn push(&mut self, x: i16) {
        self.a.push(x);
    }

    /// Pop the last item off the vector
    pub fn pop(&mut self) -> Option<i16> {
        self.a.pop()
    }
}

/// Expose `Vector::push()` to the host
#[no_mangle]
unsafe fn push(arg_len: u32) -> u32 {
    uplink::wrap_transaction(arg_len, |arg| STATE.push(arg))
}

/// Expose `Vector::pop()` to the host
#[no_mangle]
unsafe fn pop(arg_len: u32) -> u32 {
    uplink::wrap_transaction(arg_len, |_arg: ()| STATE.pop())
}
