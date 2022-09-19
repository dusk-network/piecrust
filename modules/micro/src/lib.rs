// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

#![no_std]
#![no_main]

#[allow(unused)]
use uplink;

static mut A: u32 = 42;

#[no_mangle]
unsafe fn change(to: u32) -> u32 {
    let r = A;
    A = to;
    r
}
