// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

#![no_std]
#![no_main]

use nstack::annotation::Cardinality;
use nstack::NStack;
use ranno::Annotation;

#[global_allocator]
static ALLOCATOR: dallo::HostAlloc = dallo::HostAlloc;

#[derive(Default)]
pub struct Stack {
    inner: NStack<i32, Cardinality>,
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

    pub fn len(&self) -> i32 {
        *Cardinality::from_child(&self.inner) as i32
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

#[no_mangle]
unsafe fn len(a: i32) -> i32 {
    dallo::wrap_query(&mut A, a, |_arg: ()| SELF.len())
}
