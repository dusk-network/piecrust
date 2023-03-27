// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

#![feature(alloc_error_handler, lang_items)]
#![no_std]

extern crate alloc;

mod snap;

pub use snap::snap;

mod state;
pub use state::{
    caller, height, host_query, limit, meta_data, query, query_raw, spent,
    ModuleError, State,
};

mod helpers;
pub use helpers::*;

mod ops;
pub use ops::*;

mod types;
pub use types::*;

pub mod bufwriter;
pub mod debug;

/// How many bytes to use for scratch space when serializing
pub const SCRATCH_BUF_BYTES: usize = 64;

/// The size of the argument buffer in bytes
pub const ARGBUF_LEN: usize = 64 * 1024;

#[cfg(not(feature = "std"))]
mod handlers;
