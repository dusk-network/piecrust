// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.
#![no_std]
#![no_main]

use dallo::Vec;

pub struct Vector {
    a: Vec<i16>,
}

static mut SELF: Vector = Vector { a: Vec::new() };

impl Vector {
    pub fn push(&mut self, x: i16) {
        self.a.push(x)
    }

    pub fn pop(&mut self) -> Option<i16> {
        self.a.pop()
    }
}

#[no_mangle]
fn push(x: i16) {
    unsafe { SELF.push(x) }
}

#[no_mangle]
fn pop() -> Option<i16> {
    unsafe { SELF.pop() }
}
