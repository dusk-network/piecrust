// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.
#![no_std]
#![no_main]

extern crate alloc;

use alloc::boxed::Box;

use dallo::HostAlloc;
#[global_allocator]
static ALLOCATOR: HostAlloc = HostAlloc;

// One Box, many `Boxen`
pub struct Boxen {
    a: Option<Box<i16>>,
    #[allow(unused)]
    b: i16,
}

const ARGBUF_LEN: usize = 6;

#[no_mangle]
static mut A: [u8; ARGBUF_LEN] = [0u8; ARGBUF_LEN];
#[no_mangle]
static AL: i32 = ARGBUF_LEN as i32;

static mut SELF: Boxen = Boxen { a: None, b: 0xbb };

impl Boxen {
    pub fn set(&mut self, x: i16) {
        dallo::snap();
        match self.a.as_mut() {
            Some(o) => **o = x,
            None => self.a = Some(Box::new(x)),
        }
        dallo::snap();
    }

    pub fn get(&self) -> Option<i16> {
        self.a.as_ref().map(|i| **i)
    }
}

#[no_mangle]
unsafe fn set(a: i32) -> i32 {
    dallo::wrap_transaction(&mut A, a, |to| SELF.set(to))
}

#[no_mangle]
unsafe fn get(a: i32) -> i32 {
    dallo::wrap_transaction(&mut A, a, |_: ()| SELF.get())
}
