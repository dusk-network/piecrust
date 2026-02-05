// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Contract that implements a simple nstack.

#![no_std]

use nstack::annotation::Cardinality;
use nstack::NStack;
use ranno::Annotation;

use piecrust_uplink as uplink;

/// Struct that describes the state of the stack contract
pub struct Stack {
    inner: NStack<i32, Cardinality>,
}

/// State of the stack contract
static mut STATE: Stack = Stack {
    inner: NStack::new(),
};

impl Stack {
    /// Push a new item onto the stack
    pub fn push(&mut self, elem: i32) {
        self.inner.push(elem);
    }

    /// Pop the latest item off the stack
    pub fn pop(&mut self) -> Option<i32> {
        self.inner.pop()
    }

    /// Return the length of the stack
    pub fn len(&self) -> u32 {
        *Cardinality::from_child(&self.inner) as u32
    }
}

/// Expose `Stack::push()` to the host
#[unsafe(no_mangle)]
unsafe fn push(arg_len: u32) -> u32 {
    // SAFETY: WASM smart contracts are single-threaded, so accessing mutable
    // static via raw pointer is safe - there's no risk of data races.
    unsafe {
        uplink::wrap_call(arg_len, |elem: i32| (*(&raw mut STATE)).push(elem))
    }
}

/// Expose `Stack::pop()` to the host
#[unsafe(no_mangle)]
unsafe fn pop(arg_len: u32) -> u32 {
    // SAFETY: WASM smart contracts are single-threaded, so accessing mutable
    // static via raw pointer is safe - there's no risk of data races.
    unsafe { uplink::wrap_call(arg_len, |_arg: ()| (*(&raw mut STATE)).pop()) }
}

/// Expose `Stack::len()` to the host
#[unsafe(no_mangle)]
unsafe fn len(arg_len: u32) -> u32 {
    // SAFETY: WASM smart contracts are single-threaded, so accessing mutable
    // static via raw pointer is safe - there's no risk of data races.
    unsafe { uplink::wrap_call(arg_len, |_arg: ()| (*(&raw const STATE)).len()) }
}
