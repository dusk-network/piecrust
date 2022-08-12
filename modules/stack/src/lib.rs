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

use dallo::{HostAlloc, ModuleId, State};

#[global_allocator]
static ALLOCATOR: HostAlloc = HostAlloc;

#[derive(Default)]
pub struct Stack {
    inner: NStack<i32, Cardinality>,
}

const ARGBUF_LEN: usize = 8;

#[no_mangle]
static mut A: [u64; ARGBUF_LEN / 8] = [0; ARGBUF_LEN / 8];
#[no_mangle]
static AL: u32 = ARGBUF_LEN as u32;

#[no_mangle]
static SELF_ID: ModuleId = ModuleId::uninitialized();

static mut STATE: State<Stack> = State::new(
    Stack {
        inner: NStack::new(),
    },
    unsafe { &mut A },
);

impl Stack {
    pub fn push(&mut self, elem: i32) {
        self.inner.push(elem);
    }

    pub fn pop(&mut self) -> Option<i32> {
        self.inner.pop()
    }

    pub fn len(&self) -> u32 {
        *Cardinality::from_child(&self.inner) as u32
    }
}

#[no_mangle]
unsafe fn push(arg_len: u32) -> u32 {
    dallo::wrap_transaction(STATE.buffer(), arg_len, |elem: i32| {
        STATE.push(elem)
    })
}

#[no_mangle]
unsafe fn pop(arg_len: u32) -> u32 {
    dallo::wrap_transaction(STATE.buffer(), arg_len, |_arg: ()| STATE.pop())
}

#[no_mangle]
unsafe fn len(arg_len: u32) -> u32 {
    dallo::wrap_query(STATE.buffer(), arg_len, |_arg: ()| STATE.len())
}
