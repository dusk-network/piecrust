#![feature(alloc_error_handler, lang_items, const_mut_refs)]
#![no_std]

extern crate alloc;
use alloc::vec::Vec;

mod snap;

pub use snap::snap;

#[cfg(feature = "wasm")]
mod handlers;

mod helpers;
pub use helpers::*;

#[cfg(feature = "wasm")]
mod host_allocator;
#[cfg(feature = "wasm")]
pub use host_allocator::HostAlloc;

pub type ModuleId = [u8; 32];

pub type RawQuery = Vec<u8>;
pub type RawTransaction = Vec<u8>;
pub type ReturnBuf = Vec<u8>;
