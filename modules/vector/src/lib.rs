// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

#![no_std]
#![no_main]
extern crate alloc;

use dallo::{HostAlloc, State};

#[global_allocator]
static ALLOCATOR: HostAlloc = HostAlloc;

use alloc::vec::Vec;

pub struct Vector {
    a: Vec<i16>,
}

const ARGBUF_LEN: usize = 8;

#[no_mangle]
static mut A: [u64; ARGBUF_LEN / 8] = [0; ARGBUF_LEN / 8];
#[no_mangle]
static AL: i32 = ARGBUF_LEN as i32;

static mut STATE: State<Vector> =
    unsafe { State::new(Vector { a: Vec::new() }, &mut A) };

impl Vector {
    pub fn push(&mut self, x: i16) {
        self.a.push(x);
    }

    pub fn pop(&mut self) -> Option<i16> {
        self.a.pop()
    }
}

#[no_mangle]
unsafe fn push(a: i32) -> i32 {
    dallo::wrap_transaction(STATE.buffer(), a, |arg| STATE.push(arg))
}

#[no_mangle]
unsafe fn pop(a: i32) -> i32 {
    dallo::wrap_transaction(STATE.buffer(), a, |_arg: ()| STATE.pop())
}
