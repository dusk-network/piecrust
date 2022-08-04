// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

#![feature(arbitrary_self_types)]
#![no_std]
#![no_main]

use dallo::{wrap_query, wrap_transaction, HostAlloc, ModuleId, State};

#[global_allocator]
static ALLOCATOR: HostAlloc = HostAlloc;

#[derive(Default)]
pub struct Callcenter;

const ARGBUF_LEN: usize = 2048;

#[no_mangle]
static mut A: [u64; ARGBUF_LEN / 8] = [0; ARGBUF_LEN / 8];
#[no_mangle]
static AL: i32 = ARGBUF_LEN as i32;

#[no_mangle]
static SELF_ID: ModuleId = ModuleId::uninitialized();

static mut STATE: State<Callcenter> = unsafe { State::new(Callcenter, &mut A) };

impl Callcenter {
    pub fn query_counter(self: &State<Self>, counter_id: ModuleId) -> i64 {
        self.query(counter_id, "read_value", ())
    }

    pub fn increment_counter(self: &mut State<Self>, counter_id: ModuleId) {
        self.emit(counter_id);
        self.transact(counter_id, "increment", ())
    }

    pub fn calling_self(self: &mut State<Self>, id: ModuleId) -> bool {
        dallo::self_id() == id
    }
}

#[no_mangle]
unsafe fn query_counter(a: i32) -> i32 {
    wrap_query(STATE.buffer(), a, |counter_id| {
        STATE.query_counter(counter_id)
    })
}

#[no_mangle]
unsafe fn increment_counter(a: i32) -> i32 {
    wrap_transaction(STATE.buffer(), a, |counter_id| {
        STATE.increment_counter(counter_id)
    })
}

#[no_mangle]
unsafe fn calling_self(a: i32) -> i32 {
    wrap_query(STATE.buffer(), a, |self_id| STATE.calling_self(self_id))
}
