// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Contract to perform a simple host call.

#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use piecrust_uplink as uplink;

/// Struct that describes the state of the host contract
pub struct Hoster;

/// State of the host contract
static mut STATE: Hoster = Hoster;

impl Hoster {
    /// Call 'hash' function via the host
    pub fn host_hash(&self, bytes: Vec<u8>) -> [u8; 32] {
        uplink::host_query("hash", bytes)
    }
}

/// Expose `Hoster::host_hash()` to the host
#[no_mangle]
unsafe fn host_hash(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |num| STATE.host_hash(num))
}
