// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

#![feature(arbitrary_self_types)]
#![no_std]
#![no_main]

#[derive(Default)]
pub struct Fibonacci;

use piecrust_uplink as uplink;
use uplink::{ModuleId, State};

#[no_mangle]
static SELF_ID: ModuleId = ModuleId::uninitialized();

#[allow(unused)]
static mut STATE: State<Fibonacci> = State::new(Fibonacci);

impl Fibonacci {
    fn nth(n: u32) -> u64 {
        match n {
            0 | 1 => 1,
            n => Self::nth(n - 1) + Self::nth(n - 2),
        }
    }
}

#[no_mangle]
unsafe fn nth(arg_len: u32) -> u32 {
    uplink::wrap_query(arg_len, |n: u32| Fibonacci::nth(n))
}
