// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;
use piecrust_uplink::{ModuleId, State};

pub struct Hoster;

#[no_mangle]
static SELF_ID: ModuleId = ModuleId::uninitialized();

static mut STATE: State<Hoster> = State::new(Hoster);

impl Hoster {
    pub fn hash(&self, bytes: Vec<u8>) -> [u8; 32] {
        piecrust_uplink::host_query("hash", bytes)
    }
}

#[no_mangle]
unsafe fn hash(arg_len: u32) -> u32 {
    piecrust_uplink::wrap_query(arg_len, |num| STATE.hash(num))
}
