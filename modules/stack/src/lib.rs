// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

#![no_std]

use nstack::annotation::Cardinality;
use nstack::NStack;
use ranno::Annotation;

use piecrust_uplink as uplink;
use uplink::{ModuleId, State};

#[derive(Default)]
pub struct Stack {
    inner: NStack<i32, Cardinality>,
}

#[no_mangle]
static SELF_ID: ModuleId = ModuleId::uninitialized();

static mut STATE: State<Stack> = State::new(Stack {
    inner: NStack::new(),
});

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
    uplink::wrap_transaction(arg_len, |elem: i32| STATE.push(elem))
}

#[no_mangle]
unsafe fn pop(arg_len: u32) -> u32 {
    uplink::wrap_transaction(arg_len, |_arg: ()| STATE.pop())
}

#[no_mangle]
unsafe fn len(arg_len: u32) -> u32 {
    uplink::wrap_query(arg_len, |_arg: ()| STATE.len())
}
