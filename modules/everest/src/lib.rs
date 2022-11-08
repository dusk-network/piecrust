// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

#![feature(arbitrary_self_types)]
#![no_std]
#![no_main]

#[derive(Default)]
pub struct Height;

use piecrust_uplink as uplink;
use uplink::{ModuleId, State};

#[no_mangle]
static SELF_ID: ModuleId = ModuleId::uninitialized();

static mut STATE: State<Height> = State::new(Height);

impl Height {
    pub fn get_height(&self) -> u64 {
        uplink::host_data::<u64>("height")
    }
}

#[no_mangle]
unsafe fn get_height(a: u32) -> u32 {
    uplink::wrap_query(a, |_: ()| STATE.get_height())
}
