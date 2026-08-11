// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Contract used to exercise the host-backed ABI.

#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use piecrust_uplink::wrap_call;

/// Accept a dynamically sized initializer.
#[unsafe(no_mangle)]
pub extern "C" fn init(arg_len: u32) -> u32 {
    wrap_call(arg_len, |_: Vec<u8>| ())
}

/// Return the input unchanged.
#[unsafe(no_mangle)]
pub extern "C" fn echo(arg_len: u32) -> u32 {
    wrap_call(arg_len, |input: Vec<u8>| input)
}
