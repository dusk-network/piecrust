// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

#![feature(alloc_error_handler, lang_items)]
#![no_std]

extern crate alloc;

/// How many bytes to use for scratch space when serializing
pub const SCRATCH_BUF_BYTES: usize = 64;

/// The size of the argument buffer in bytes
pub const ARGBUF_LEN: usize = 64 * 1024;

#[cfg(not(feature = "std"))]
mod snap;
#[cfg(not(feature = "std"))]
pub use snap::snap;

#[cfg(not(feature = "std"))]
mod state;
#[cfg(not(feature = "std"))]
pub use state::{
    caller, height, host_data, host_query, limit, query, query_raw, spent,
    State,
};

#[cfg(not(feature = "std"))]
mod helpers;
#[cfg(not(feature = "std"))]
pub use helpers::*;

#[cfg(not(feature = "std"))]
mod ops;
#[cfg(not(feature = "std"))]
pub use ops::*;

mod types;
pub use types::*;

#[cfg(not(feature = "std"))]
pub mod bufwriter;
#[cfg(not(feature = "std"))]
pub mod debug;

#[cfg(not(feature = "std"))]
mod handlers;
