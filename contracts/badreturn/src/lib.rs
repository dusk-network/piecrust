// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Contract that deliberately returns malformed (non-rkyv) bytes.
//! Used exclusively for testing that callers handle deserialization
//! failures gracefully (receiving `Err` instead of panicking).

#![no_std]

use piecrust_uplink as uplink;
use uplink::wrap_call;

/// Struct that describes the state of the BadReturn contract
pub struct BadReturn;

/// State of the BadReturn contract
static mut STATE: BadReturn = BadReturn;

impl BadReturn {
    /// Returns a valid bool value, serialized correctly via rkyv.
    pub fn valid_bool(&self) -> bool {
        true
    }
}

/// Expose `BadReturn::valid_bool()`. Returns correctly serialized bool.
#[unsafe(no_mangle)]
unsafe fn valid_bool(arg_len: u32) -> u32 {
    unsafe {
        wrap_call(arg_len, |_: ()| (*(&raw const STATE)).valid_bool())
    }
}

/// Returns garbage bytes that are NOT a valid rkyv archive for types with
/// validation constraints. This bypasses `wrap_call` entirely and writes
/// raw invalid bytes into the argument buffer.
#[unsafe(no_mangle)]
unsafe fn garbage_value(_arg_len: u32) -> u32 {
    piecrust_uplink::arg_buf::with_arg_buf(|buf| {
        // Write garbage bytes
        let garbage: [u8; 16] = [
            0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xD0, 0xFF, 0xFF,
            0xFF, 0xFF, 0x99, 0x99, 0x99, 0x99,
        ];
        buf[..garbage.len()].copy_from_slice(&garbage);
        garbage.len() as u32
    })
}
