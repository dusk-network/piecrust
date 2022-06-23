#![feature(alloc_error_handler, lang_items, const_mut_refs)]
#![cfg_attr(not(feature = "host"), no_std)]

#[cfg(not(feature = "host"))]
mod guest_mem;

#[cfg(feature = "host")]
mod host_mem;
mod memory;

mod boxed;
mod vec;

pub use boxed::Box;
pub use vec::Vec;

#[cfg(not(feature = "host"))]
mod handlers;
