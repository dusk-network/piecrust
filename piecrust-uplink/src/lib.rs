// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Piecrust Uplink is the crate for writing WASM smart contracts in Rust
//! targeting the Piecrust virtual machine.
//!
//! A smart contract is a program exposing a collection of functions that
//! operate on a shared state. These functions can perform a variety of
//! operations, such as reading or modifying the state, or interacting with the
//! host.
//!
//! # State Model and ABI
//! Contracts targeting the Piecrust VM represent their state as a [WASM
//! memory]. The contract is free to represent its data as it sees fit, and may
//! allocate at will.
//!
//! To communicate with the host, both the contract and the host can
//! emplace data in a special region of this memory called the argument buffer.
//! The argument buffer is used as both the input from the host and as the
//! output of the contract. All functions exposed by the contract must follow
//! the convention:
//!
//! ```c
//! // A function compatible with the piecrust ABI takes in the number of bytes
//! // written by the host to the argument buffer and returns the number of
//! // bytes written by the contract.
//! uint32_t fname(uint32_t arg_len);
//! ```
//!
//! The contract also has some functions available to it, offered by the host
//! through WASM imports. Examples of these functions include, but are not
//! limited to:
//!
//! - [`call`] to call another contract
//! - [`emit`] to emit events
//!
//! The functions in this crate are wrappers around a particular way of calling
//! the WASM imports. Take a look at the [externs] for a full view of what is
//! available.
//!
//! # Examples
//! Some of the contracts used for testing purposes function as good examples.
//! Take a look at the [contracts/] directory.
//!
//! # Features
//! By default, this crate will include no features and build only the types and
//! functions available when the ABI is not present. To write a contract one
//! must use the `abi` feature:
//!
//! - `abi` for writing contracts
//! - `dlmalloc` to using the builtin allocator
//! - `debug` for writing contracts with debug capabilities such as the
//!   [`debug!`] macro, and logging panics to stdout
//!
//! [WASM memory]: https://wasmbyexample.dev/examples/webassembly-linear-memory/webassembly-linear-memory.rust.en-us.html
//! [contracts/]: https://github.com/dusk-network/piecrust/tree/main/contracts
//! [externs]: https://github.com/dusk-network/piecrust/blob/c2dadaa8dec210bdbbc72619a687eb8c6f693877/piecrust-uplink/src/abi/state.rs#L42-L64

#![allow(internal_features)]
#![feature(lang_items, panic_info_message)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![no_std]

extern crate alloc;

#[cfg(feature = "abi")]
#[cfg_attr(docsrs, doc(cfg(feature = "abi")))]
mod abi;
#[cfg(feature = "abi")]
pub use abi::*;

mod types;
pub use types::*;

mod error;
pub use error::*;

/// How many bytes to use for scratch space when serializing
pub const SCRATCH_BUF_BYTES: usize = 1024;

/// The size of the argument buffer in bytes
pub const ARGBUF_LEN: usize = 64 * 1024;
