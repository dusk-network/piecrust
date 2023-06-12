// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

#![feature(lang_items)]
#![no_std]

extern crate alloc;

mod state;
pub use state::{
    call, call_raw, call_raw_with_limit, call_with_limit, caller, emit,
    host_query, limit, meta_data, owner, self_id, spent,
};

mod helpers;
pub use helpers::*;

mod types;
pub use types::*;

mod error;
pub use error::*;

#[cfg(feature = "debug")]
pub mod debug;
#[cfg(feature = "debug")]
pub use debug::*;

/// How many bytes to use for scratch space when serializing
pub const SCRATCH_BUF_BYTES: usize = 64;

/// The size of the argument buffer in bytes
pub const ARGBUF_LEN: usize = 64 * 1024;

#[cfg(not(feature = "std"))]
mod handlers;
