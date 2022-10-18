// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

#![feature(arbitrary_self_types)]
#![no_std]
#![no_main]

#[derive(Default)]
pub struct Spender;

use piecrust_uplink::{ModuleId, State};

#[no_mangle]
static SELF_ID: ModuleId = ModuleId::uninitialized();

static mut STATE: State<Spender> = State::new(Spender);

impl Spender {
    pub fn get_limit_and_spent(&self) -> (u64, u64, u64, u64, u64) {
        let self_id = piecrust_uplink::self_id();

        let limit = piecrust_uplink::limit();
        let spent_before = piecrust_uplink::spent();

        match piecrust_uplink::caller().is_uninitialized() {
            true => {
                let (called_limit, called_spent, _, _, _): (
                    u64,
                    u64,
                    u64,
                    u64,
                    u64,
                ) = piecrust_uplink::query(self_id, "get_limit_and_spent", ());

                let spent_after = piecrust_uplink::spent();
                (limit, spent_before, spent_after, called_limit, called_spent)
            }
            false => (limit, spent_before, 0, 0, 0),
        }
    }
}

#[no_mangle]
unsafe fn get_limit_and_spent(a: u32) -> u32 {
    piecrust_uplink::wrap_query(a, |_: ()| STATE.get_limit_and_spent())
}
