// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Contract to get the current block height from the host.

#![no_std]

use piecrust_uplink as uplink;

/// Struct that describes the state of the everest contract
pub struct Height;

/// State of the everest contract
static mut STATE: Height = Height;

impl Height {
    /// Query the host for the current block height
    pub fn get_height(&self) -> Option<u64> {
        uplink::meta_data::<u64>("height")
    }
}

/// Expose `Height::get_height()` to the host
#[no_mangle]
unsafe fn get_height(a: u32) -> u32 {
    uplink::wrap_call(a, |_: ()| STATE.get_height())
}
