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
pub struct Callcenter;

use dallo::*;

const ARGBUF_LEN: usize = 1024;

#[no_mangle]
static mut A: [u8; ARGBUF_LEN] = [0u8; ARGBUF_LEN];
#[no_mangle]
static AL: i32 = ARGBUF_LEN as i32;

static mut SELF: State<Callcenter> = unsafe { State::new(Callcenter, &mut A) };

const COUNTER_ID: &[u8; 32] = include_bytes!("../../counter/id");

impl Callcenter {
    pub fn query_counter(self: &State<Self>) -> i64 {
        self.query(*COUNTER_ID, "read_value", ())
    }

    pub fn increment_counter(self: &mut State<Self>) {
        self.transact(*COUNTER_ID, "increment", ())
    }
}

#[no_mangle]
unsafe fn query_counter(a: i32) -> i32 {
    dallo::wrap_query(&mut A, a, |_: ()| SELF.query_counter())
}

#[no_mangle]
unsafe fn increment_counter(a: i32) -> i32 {
    dallo::wrap_transaction(&mut A, a, |_: ()| SELF.increment_counter())
}
