#![feature(alloc_error_handler, lang_items, const_mut_refs)]
#![no_std]

extern crate alloc;
use alloc::vec::Vec;
use core::ops::{Deref, DerefMut};

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

pub struct State<S>(S);

impl<S> State<S> {
    pub const fn new(s: S) -> Self {
        State(s)
    }
}

impl<S> Deref for State<S> {
    type Target = S;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S> DerefMut for State<S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<S> State<S> {
    pub fn query<A, R>(&self, mod_id: ModuleId, name: &'static str, arg: A) -> R {
        todo!()
    }

    pub fn transact<A, R>(&mut self, mod_id: ModuleId, name: &'static str, arg: A) -> R {
        todo!();
    }
}
