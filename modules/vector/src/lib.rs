// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

#![no_std]
#![no_main]
extern crate alloc;

use dallo::{HostAlloc, ModuleId, State};

#[global_allocator]
static ALLOCATOR: HostAlloc = HostAlloc;

use alloc::vec::Vec;

pub struct Vector {
    a: Vec<i16>,
}

#[no_mangle]
static SELF_ID: ModuleId = ModuleId::uninitialized();

static mut STATE: State<Vector> = State::new(Vector { a: Vec::new() });

impl Vector {
    pub fn push(&mut self, x: i16) {
        self.a.push(x);
    }

    pub fn pop(&mut self) -> Option<i16> {
        self.a.pop()
    }
}

#[no_mangle]
unsafe fn push(arg_len: u32) -> u32 {
    dallo::wrap_transaction(arg_len, |arg| STATE.push(arg))
}

#[no_mangle]
unsafe fn pop(arg_len: u32) -> u32 {
    dallo::wrap_transaction(arg_len, |_arg: ()| STATE.pop())
}
