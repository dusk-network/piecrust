// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

#![feature(arbitrary_self_types)]
#![no_std]
#![no_main]

extern crate alloc;

#[derive(Default)]
pub struct Debug;

use piecrust_uplink::{ModuleId, State};

#[no_mangle]
static SELF_ID: ModuleId = ModuleId::uninitialized();

static mut STATE: State<Debug> = State::new(Debug);

impl Debug {
    pub fn debug(&self, string: alloc::string::String) {
        piecrust_uplink::debug!("What a string! {}", string);
    }

    pub fn panic(&self) {
        panic!("It's never too late to panic");
    }
}

#[no_mangle]
unsafe fn debug(arg_len: u32) -> u32 {
    piecrust_uplink::wrap_query(arg_len, |s: alloc::string::String| {
        STATE.debug(s)
    })
}
