// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Contract which exposes the call stack

#![no_std]

extern crate alloc;

use piecrust_uplink as uplink;
use alloc::vec::Vec;
use uplink::ContractId;

/// Struct that describes the state of the contract
pub struct CallStack;

/// State of the Counter contract
static mut STATE: CallStack = CallStack;

impl CallStack {
    /// Return the call stack
    pub fn return_callstack(&self) -> Vec<ContractId> {
        uplink::callstack()
    }
}

/// Expose `CallStack::read_callstack()` to the host
#[no_mangle]
unsafe fn return_callstack(arg_len: u32) -> u32 {
    uplink::wrap_call_unchecked(arg_len, |_: ()| STATE.return_callstack())
}
