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
pub struct Height;

use dallo::{ModuleId, State};

const ARGBUF_LEN: usize = 64;

#[no_mangle]
static mut A: [u64; ARGBUF_LEN / 8] = [0; ARGBUF_LEN / 8];
#[no_mangle]
static AL: i32 = ARGBUF_LEN as i32;

#[no_mangle]
static SELF_ID: ModuleId = ModuleId::uninitialized();

static mut STATE: State<Height> = unsafe { State::new(Height, &mut A) };

impl Height {
    pub fn get_height(self: &State<Height>) -> u64 {
        self.height()
    }
}

#[no_mangle]
unsafe fn get_height(a: i32) -> i32 {
    dallo::wrap_query(STATE.buffer(), a, |_: ()| STATE.get_height())
}
