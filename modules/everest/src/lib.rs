// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Module to get the current block height from the host.

#![feature(arbitrary_self_types)]
#![no_std]

use piecrust_uplink as uplink;
use uplink::{ModuleId, State};

/// Struct that describes the state of the everest module
pub struct Height;

/// Module ID, initialized by the host when the module is deployed
#[no_mangle]
static SELF_ID: ModuleId = ModuleId::uninitialized();

/// State of the everest module
static mut STATE: State<Height> = State::new(Height);

impl Height {
    /// Query the host for the current block height
    pub fn get_height(&self) -> u64 {
        uplink::host_data::<u64>("height")
    }
}

/// Expose `Height::get_height()` to the host
#[no_mangle]
unsafe fn get_height(a: u32) -> u32 {
    uplink::wrap_query(a, |_: ()| STATE.get_height())
}
