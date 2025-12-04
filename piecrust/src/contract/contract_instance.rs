// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use crate::Error;
use std::io;

pub trait ContractInstance: Send {
    fn snap(&mut self) -> io::Result<()>;

    fn revert(&mut self) -> io::Result<()>;

    fn apply(&mut self) -> io::Result<()>;

    // Write argument into instance
    fn write_argument(&mut self, arg: &[u8]);

    // Read argument from instance
    fn read_argument(&mut self, arg: &mut [u8]);

    fn read_bytes_from_arg_buffer(&self, arg_len: u32) -> Vec<u8>;

    fn with_memory<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R;

    fn with_memory_mut<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R;

    /// Returns the current length of the memory.
    fn mem_len(&self) -> usize;

    /// Sets the length of the memory.
    fn set_len(&mut self, len: usize);

    fn with_arg_buf<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R;

    fn with_arg_buf_mut<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R;

    fn write_bytes_to_arg_buffer(&mut self, buf: &[u8]) -> Result<u32, Error>;

    fn set_remaining_gas(&mut self, limit: u64);

    fn get_remaining_gas(&mut self) -> u64;

    fn is_function_exported<N: AsRef<str>>(&mut self, name: N) -> bool;

    #[allow(dead_code)]
    fn print_state(&self);
    fn arg_buffer_offset(&self) -> usize;

    fn map_call_err(&mut self, err: dusk_wasmtime::Error) -> Error;

    fn call(
        &mut self,
        method_name: &str,
        arg_len: u32,
        limit: u64,
    ) -> Result<i32, Error>;
}
