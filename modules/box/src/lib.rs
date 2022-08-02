// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::boxed::Box;

use dallo::{HostAlloc, State, MODULE_ID_BYTES};
#[global_allocator]
static ALLOCATOR: HostAlloc = HostAlloc;

// One Box, many `Boxen`
pub struct Boxen {
    a: Option<Box<i16>>,
    #[allow(unused)]
    b: i16,
}

const ARGBUF_LEN: usize = 8;

#[no_mangle]
static mut A: [u64; ARGBUF_LEN / 8] = [0; ARGBUF_LEN / 8];
#[no_mangle]
static AL: i32 = ARGBUF_LEN as i32;

#[no_mangle]
static CALLER: [u8; MODULE_ID_BYTES + 1] = [0u8; MODULE_ID_BYTES + 1];
#[no_mangle]
static SELF_ID: [u8; MODULE_ID_BYTES] = [0u8; MODULE_ID_BYTES];

static mut STATE: State<Boxen> =
    unsafe { State::new(Boxen { a: None, b: 0xbb }, &mut A) };

impl Boxen {
    pub fn set(&mut self, x: i16) {
        match self.a.as_mut() {
            Some(o) => **o = x,
            None => self.a = Some(Box::new(x)),
        }
    }

    pub fn get(&self) -> Option<i16> {
        self.a.as_ref().map(|i| **i)
    }
}

#[no_mangle]
unsafe fn set(a: i32) -> i32 {
    dallo::wrap_transaction(STATE.buffer(), a, |to| STATE.set(to))
}

#[no_mangle]
unsafe fn get(a: i32) -> i32 {
    dallo::wrap_transaction(STATE.buffer(), a, |_: ()| STATE.get())
}
