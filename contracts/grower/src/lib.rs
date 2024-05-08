// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Contract keeping a vector of bytes, and allowing the user to append to it,
//! and query for "views" into said vector.
//!
//! This contract does *not* use `rkyv` to serialize to and deserialize from the
//! argument buffer. Instead it uses its own little protocol, writing bytes
//! directly onto the argument buffer.

#![no_std]

extern crate alloc;

use alloc::vec::Vec;

use piecrust_uplink as uplink;
use uplink::arg_buf::with_arg_buf;

/// State of the grower contract - a vector of bytes.
struct Grower(Vec<u8>);

/// State of the grower contract
static mut STATE: Grower = Grower(Vec::new());

impl Grower {
    /// Appends the bytes sent in the argument buffer to the state vector.
    fn append(&mut self, len: usize) {
        with_arg_buf(|buf, _| {
            self.0.extend_from_slice(&buf[..len]);
        });
    }

    /// Parses offset and length from the argument buffer, and copies the bytes
    /// at said offset and length on the state vector to the argument buffer.
    fn view(&self, arg_len: usize) -> usize {
        with_arg_buf(|buf, _| {
            if arg_len != 8 {
                panic!("Bad arguments");
            }

            let offset_slice = &buf[..4];
            let len_slice = &buf[4..8];

            let mut offset_bytes = [0; 4];
            let mut len_bytes = [0; 4];

            offset_bytes.copy_from_slice(offset_slice);
            len_bytes.copy_from_slice(len_slice);

            let offset = u32::from_le_bytes(offset_bytes) as usize;
            let len = u32::from_le_bytes(len_bytes) as usize;

            buf[..len].copy_from_slice(&self.0[offset..][..len]);

            len
        })
    }

    /// Emplace the length of the state vector into the argument buffer.
    fn len(&self) -> usize {
        with_arg_buf(|buf, _| {
            let len = self.0.len() as u32;
            let len_bytes = len.to_le_bytes();
            buf[..4].copy_from_slice(&len_bytes);
            4
        })
    }
}

/// Expose `Grower::append()` to the host
#[no_mangle]
unsafe fn append(arg_len: u32) -> u32 {
    STATE.append(arg_len as usize);
    0
}

/// Expose `Grower::append()` to the host, but panic afterwards.
#[no_mangle]
unsafe fn append_and_panic(arg_len: u32) -> u32 {
    STATE.append(arg_len as usize);
    panic!("isded");
}

/// Expose `Grower::view()` to the host
#[no_mangle]
unsafe fn view(arg_len: u32) -> u32 {
    STATE.view(arg_len as usize) as u32
}

/// Expose `Grower::len()` to the host
#[no_mangle]
unsafe fn len(_arg_len: u32) -> u32 {
    STATE.len() as u32
}
