// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

#![feature(arbitrary_self_types)]
#![no_std]
#![no_main]

#[global_allocator]
static ALLOCATOR: dallo::HostAlloc = dallo::HostAlloc;

#[derive(Default)]
pub struct Fibonacci;

use dallo::{State, MODULE_ID_BYTES};

const ARGBUF_LEN: usize = 64;

#[no_mangle]
static mut A: [u64; ARGBUF_LEN / 8] = [0; ARGBUF_LEN / 8];

#[no_mangle]
static AL: i32 = ARGBUF_LEN as i32;

#[no_mangle]
static CALLER: [u8; MODULE_ID_BYTES + 1] = [0u8; MODULE_ID_BYTES + 1];
#[no_mangle]
static SELF_ID: [u8; MODULE_ID_BYTES] = [0u8; MODULE_ID_BYTES];

#[allow(unused)]
static mut STATE: State<Fibonacci> = State::new(Fibonacci, unsafe { &mut A });

impl Fibonacci {
    fn nth(n: u32) -> u64 {
        dallo::snap();
        match n {
            0 | 1 => 1,
            n => Self::nth(n - 1) + Self::nth(n - 2),
        }
    }
}

#[no_mangle]
unsafe fn nth(a: i32) -> i32 {
    dallo::wrap_query(STATE.buffer(), a, |n: u32| Fibonacci::nth(n))
}
