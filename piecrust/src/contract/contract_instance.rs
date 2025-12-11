// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use crate::Error;
use std::fmt;
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

    /// Returns the current length of the memory.
    fn mem_len(&self) -> usize;

    /// Sets the length of the memory.
    fn set_len(&mut self, len: usize);

    fn write_bytes_to_arg_buffer(&mut self, buf: &[u8]) -> Result<u32, Error>;

    fn set_remaining_gas(&mut self, limit: u64);

    fn get_remaining_gas(&mut self) -> u64;

    fn is_function_exported(&mut self, name: &str) -> bool;

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

    fn get_memory(&self) -> &[u8];
    fn get_memory_mut(&mut self) -> &mut [u8];
    fn get_arg_buf_ofs(&self) -> usize;
}

impl fmt::Debug for dyn ContractInstance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ContractInstance")
            .field("memory_len", &self.mem_len())
            // .field("remaining_gas", &self.get_remaining_gas())
            // todo: more fields
            .finish()
    }
}

pub struct InstanceUtil;

impl InstanceUtil {
    // PROB
    pub fn with_memory<F, R>(mem: &[u8], f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        f(mem)
    }

    // PROB
    pub fn with_memory_mut<F, R>(mem: &mut [u8], f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        f(mem)
    }

    // PROB
    pub fn with_arg_buf<F, R>(mem: &[u8], arg_buf_ofs: usize, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        InstanceUtil::with_memory(mem, |memory_bytes| {
            f(&memory_bytes[arg_buf_ofs..][..piecrust_uplink::ARGBUF_LEN])
        })
    }

    // PROB
    pub fn with_arg_buf_mut<F, R>(mem: &mut [u8], arg_buf_ofs: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        InstanceUtil::with_memory_mut(mem, |memory_bytes| {
            f(&mut memory_bytes[arg_buf_ofs..][..piecrust_uplink::ARGBUF_LEN])
        })
    }
}
