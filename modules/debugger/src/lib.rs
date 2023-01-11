// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Simple debug module.

#![feature(arbitrary_self_types)]
#![no_std]

extern crate alloc;

use piecrust_uplink as uplink;
use uplink::{ModuleId, State};

/// Struct that describes the state of the debug module
pub struct Debug;

/// Module id, initialized by the host when the module is deployed
#[no_mangle]
static SELF_ID: ModuleId = ModuleId::uninitialized();

/// State of the debug module
static mut STATE: State<Debug> = State::new(Debug);

impl Debug {
    /// Print debug information
    pub fn debug(&self, string: alloc::string::String) {
        uplink::debug!("What a string! {}", string);
    }

    /// Panic execution
    pub fn panic(&self) {
        panic!("It's never too late to panic");
    }
}

/// Expose `Debug::debug()` to the host
#[no_mangle]
unsafe fn debug(arg_len: u32) -> u32 {
    uplink::wrap_query(arg_len, |s: alloc::string::String| STATE.debug(s))
}
