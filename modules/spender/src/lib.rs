// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Module for testing the gas spending behavior, where the gas is measured in
//! WASM points.

#![feature(arbitrary_self_types)]
#![no_std]

use piecrust_uplink as uplink;
use uplink::State;

/// Struct that describes the state of the spender module
pub struct Spender;

/// State of the spender module
static mut STATE: State<Spender> = State::new(Spender);

impl Spender {
    /// Get the limit and spent points before and after an inter-contract call,
    /// including the limit and spent by the called contract
    pub fn get_limit_and_spent(&self) -> (u64, u64, u64, u64, u64) {
        let self_id = uplink::self_id();

        let limit = uplink::limit();
        let spent_before = uplink::spent();

        match uplink::caller().is_uninitialized() {
            // if this module has not been called by another module,
            // i.e. has been called directly from the outside, call the function
            // via the host and return the limit and spent values before and
            // after the call
            true => {
                let (called_limit, called_spent, _, _, _): (
                    u64,
                    u64,
                    u64,
                    u64,
                    u64,
                ) = uplink::query(self_id, "get_limit_and_spent", &())
                    .expect("Self query should succeed");

                let spent_after = uplink::spent();
                (limit, spent_before, spent_after, called_limit, called_spent)
            }
            // if the module has been called by another module (we do that in
            // the above match arm) only return the limit and spent at this
            // point
            false => (limit, spent_before, 0, 0, 0),
        }
    }
}

/// Expose `Spender::get_limit_and_spent()` to the host
#[no_mangle]
unsafe fn get_limit_and_spent(a: u32) -> u32 {
    uplink::wrap_query(a, |_: ()| STATE.get_limit_and_spent())
}
