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
pub struct Eventer;

use dallo::{ModuleId, State, MODULE_ID_BYTES};

const ARGBUF_LEN: usize = 64;

#[no_mangle]
static mut A: [u64; ARGBUF_LEN / 8] = [0; ARGBUF_LEN / 8];
#[no_mangle]
static AL: i32 = ARGBUF_LEN as i32;

#[no_mangle]
static SELF_ID: ModuleId = [0u8; MODULE_ID_BYTES];

static mut STATE: State<Eventer> = unsafe { State::new(Eventer, &mut A) };

impl Eventer {
    pub fn emit_num(self: &State<Eventer>, num: u32) {
        for i in 0..num {
            self.emit(i);
        }
    }
}

#[no_mangle]
unsafe fn emit_events(a: i32) -> i32 {
    dallo::wrap_query(STATE.buffer(), a, |num| STATE.emit_num(num))
}
