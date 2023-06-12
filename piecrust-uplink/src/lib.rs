// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

#![feature(lang_items)]
#![cfg_attr(feature = "debug", feature(panic_info_message))]
#![no_std]

extern crate alloc;

#[cfg(feature = "abi")]
mod abi;
#[cfg(feature = "abi")]
pub use abi::*;

mod types;
pub use types::*;

mod error;
pub use error::*;

/// How many bytes to use for scratch space when serializing
pub const SCRATCH_BUF_BYTES: usize = 64;

/// The size of the argument buffer in bytes
pub const ARGBUF_LEN: usize = 64 * 1024;
