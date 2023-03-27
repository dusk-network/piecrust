// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Module that provides and example use of the metadata.

#![feature(arbitrary_self_types)]
#![no_std]

use piecrust_uplink as uplink;
use uplink::{ModuleId, ModuleMetadata, State};

/// Struct that describes the (empty) state of the Metadata module
pub struct Metadata;


/// Module ID, initialized by the host when the module is deployed
#[no_mangle]
static SELF_ID: ModuleId = ModuleId::uninitialized();

/// State of the Metadata module
static mut STATE: State<Metadata> = State::new(Metadata {});

impl Metadata {
    /// Read the value of the metadata module state
    pub fn read_metadata(&self) -> ModuleMetadata {
        uplink::metadata()
    }
}

/// Expose `Metadata::read_owner()` to the host
#[no_mangle]
unsafe fn read_metadata(arg_len: u32) -> u32 {
    uplink::wrap_query(arg_len, |_: ()| STATE.read_metadata())
}
