// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Contract that uses the `feed` extern to report data to the host.

#![no_std]

extern crate alloc;

use piecrust_uplink as uplink;

/// Struct that describes the state of the feeder contract
pub struct Feeder;

/// State of the vector contract
static mut STATE: Feeder = Feeder;

impl Feeder {
    /// Feed the host with 32-bit integers sequentially in the `0..num` range.
    pub fn feed_num(&self, num: u32) {
        for i in 0..num {
            uplink::feed(i);
        }
    }

    /// Feed the host with 32-bit integers sequentially in the `0..num` range.
    pub fn feed_num_raw(&self, num: u32) {
        for i in 0..num {
            uplink::feed_raw(i.to_le_bytes());
        }
    }
}

/// Expose `Feeder::feed_num()` to the host
#[no_mangle]
unsafe fn feed_num(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |num| STATE.feed_num(num))
}

/// Expose `Feeder::feed_num_raw()` to the host
#[no_mangle]
unsafe fn feed_num_raw(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |num| STATE.feed_num_raw(num))
}
