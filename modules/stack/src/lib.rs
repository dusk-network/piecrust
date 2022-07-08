// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

#![no_std]
#![no_main]

use nstack::NStack;

#[global_allocator]
static ALLOCATOR: dallo::HostAlloc = dallo::HostAlloc;

#[derive(Default)]
pub struct Stack {
    inner: NStack<i32>,
}

const ARGBUF_LEN: usize = 8;

#[no_mangle]
static mut A: [u8; ARGBUF_LEN] = [0u8; ARGBUF_LEN];
#[no_mangle]
static AL: i32 = ARGBUF_LEN as i32;

static mut SELF: Stack = Stack {
    inner: NStack::new(),
};

impl Stack {
    pub fn push(&mut self, elem: i32) {
        self.inner.push(elem);
    }

    pub fn pop(&mut self) -> Option<i32> {
        self.inner.pop()
    }
}

#[no_mangle]
unsafe fn push(a: i32) -> i32 {
    dallo::wrap_transaction(&mut A, a, |elem: i32| SELF.push(elem))
}

#[no_mangle]
unsafe fn pop(a: i32) -> i32 {
    dallo::wrap_transaction(&mut A, a, |_arg: ()| SELF.pop())
}
