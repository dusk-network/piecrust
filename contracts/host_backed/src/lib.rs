// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Contract used to exercise the host-backed ABI.

#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use piecrust_uplink::{ContractId, call, host_query, wrap_call};

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

/// Catch an ordinary nested-call result so tests can verify fatal propagation.
#[unsafe(no_mangle)]
pub extern "C" fn catch_leaf_failure(arg_len: u32) -> u32 {
    wrap_call(arg_len, |leaf: ContractId| {
        let _ = call::<_, ()>(leaf, "fail_missing_query", &());
    })
}

/// Invoke an unavailable host query, which requires discarding the session.
#[unsafe(no_mangle)]
pub extern "C" fn fail_missing_query(arg_len: u32) -> u32 {
    wrap_call(arg_len, |_: ()| host_query::<_, ()>("missing-query", ()))
}
