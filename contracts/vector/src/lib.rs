// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Contract that implements the functionalities of a simple vector.

#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use piecrust_macros::contract;

/// Struct that describes the state of the vector contract
pub struct Vector {
    a: Vec<i16>,
}

/// State of the vector contract
static mut STATE: Vector = Vector { a: Vec::new() };

#[contract]
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
