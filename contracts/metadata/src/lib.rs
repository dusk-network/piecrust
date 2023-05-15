// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Contract that provides and example use of the metadata.

#![no_std]

use piecrust_uplink as uplink;
use uplink::ContractId;

/// Struct that describes the (empty) state of the Metadata contract
pub struct Metadata;

/// State of the Metadata contract
static mut STATE: Metadata = Metadata;

impl Metadata {
    /// Read the value of the contract's owner
    pub fn read_owner(&self) -> [u8; 33] {
        uplink::owner()
    }

    /// Read the value of the contract's id
    pub fn read_id(&self) -> ContractId {
        uplink::self_id()
    }
}

/// Expose `Metadata::read_owner()` to the host
#[no_mangle]
unsafe fn read_owner(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |_: ()| STATE.read_owner())
}

/// Expose `Metadata::read_id()` to the host
#[no_mangle]
unsafe fn read_id(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |_: ()| STATE.read_id())
}
