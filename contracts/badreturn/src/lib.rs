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
pub struct BadReturn {
    counter: u32,
}

/// State of the BadReturn contract
static mut STATE: BadReturn = BadReturn { counter: 72 };

impl BadReturn {
    /// Returns a valid bool value, serialized correctly via rkyv.
    pub fn valid_bool(&self) -> bool {
        true
    }

    /// Returns the counter value.
    pub fn counter(&self) -> u32 {
        self.counter
    }
}

/// Expose `BadReturn::valid_bool()`. Returns correctly serialized bool.
#[unsafe(no_mangle)]
unsafe fn valid_bool(arg_len: u32) -> u32 {
    unsafe {
        wrap_call(arg_len, |_: ()| (*(&raw const STATE)).valid_bool())
    }
}

/// Expose `BadReturn::counter()`.
#[unsafe(no_mangle)]
unsafe fn counter(arg_len: u32) -> u32 {
    unsafe { wrap_call(arg_len, |_: ()| (*(&raw const STATE)).counter()) }
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
/// Returns the first four raw argument bytes as the ABI return length.
///
/// This bypasses `wrap_call` so tests can verify that the VM rejects a
/// contract-controlled top-level return length instead of trusting it as a
/// sane arg-buffer size.
#[unsafe(no_mangle)]
unsafe fn raw_return_len(_arg_len: u32) -> i32 {
    unsafe {
        let state = &raw mut STATE;
        (*state).counter += 1;
    }

    piecrust_uplink::arg_buf::with_arg_buf(|buf| {
        let mut bytes = [0u8; 4];
        bytes.copy_from_slice(&buf[..4]);
        i32::from_le_bytes(bytes)
    })
}
