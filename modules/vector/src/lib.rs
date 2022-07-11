// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.
#![no_std]
#![no_main]
extern crate alloc;

use dallo::HostAlloc;
#[global_allocator]
static ALLOCATOR: HostAlloc = HostAlloc;

use alloc::vec::Vec;

pub struct Vector {
    a: Vec<i16>,
}

const ARGBUF_LEN: usize = 6;

#[no_mangle]
static mut A: [u8; ARGBUF_LEN] = [0u8; ARGBUF_LEN];
#[no_mangle]
static AL: i32 = ARGBUF_LEN as i32;

static mut SELF: Vector = Vector { a: Vec::new() };

impl Vector {
    pub fn push(&mut self, x: i16) {
        self.a.push(x);
        dallo::snap()
    }

    pub fn pop(&mut self) -> Option<i16> {
        self.a.pop()
    }
}

#[no_mangle]
unsafe fn push(a: i32) -> i32 {
    dallo::wrap_transaction(&mut A, a, |arg| SELF.push(arg))
}

#[no_mangle]
unsafe fn pop(a: i32) -> i32 {
    dallo::wrap_transaction(&mut A, a, |_arg: ()| SELF.pop())
}
