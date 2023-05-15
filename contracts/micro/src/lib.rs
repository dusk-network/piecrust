// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Minimal contract, allows for changing a static number and doesn't expose any
//! other functionality to the host

#![no_std]

#[allow(unused)]
use piecrust_uplink;

/// Struct representing the state of the change contract
static mut A: u32 = 42;

/// Change the number in the state and return the previous value
#[no_mangle]
unsafe fn change(to: u32) -> u32 {
    let r = A;
    A = to;
    r
}
