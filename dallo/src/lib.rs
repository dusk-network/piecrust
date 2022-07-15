#![feature(alloc_error_handler, lang_items, const_mut_refs)]
#![no_std]

extern crate alloc;

mod snap;

pub use snap::snap;

mod state;
pub use state::*;

mod helpers;
pub use helpers::*;

pub const MODULE_ID_BYTES: usize = 32;
pub type ModuleId = [u8; MODULE_ID_BYTES];

/// How many bytes to use for scratch space when serializing
pub const SCRATCH_BUF_BYTES: usize = 16;

#[cfg(feature = "wasm")]
mod handlers;
#[cfg(feature = "wasm")]
mod host_allocator;
#[cfg(feature = "wasm")]
pub use host_allocator::HostAlloc;
