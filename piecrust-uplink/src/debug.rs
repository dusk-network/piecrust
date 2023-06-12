// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use core::fmt;
use core::fmt::Write;
use core::ptr::slice_from_raw_parts_mut;

pub mod ext {
    extern "C" {
        pub fn host_debug(ofs: i32, len: u32);
    }
}

const DEBUG_BUFFER_SIZE: usize = 16 * 1024;
static mut DEBUG_BUFFER: [u8; DEBUG_BUFFER_SIZE] = [0u8; DEBUG_BUFFER_SIZE];

/// Write a string to the debug buffer and report it to the host.
pub fn debug(s: &str) -> fmt::Result {
    let mut w = DebugWriter::new();
    w.write_str(s)?;
    unsafe { ext::host_debug(w.ptr as i32, w.ofs as u32) };
    Ok(())
}

/// A small struct that can `fmt::Write` to the debug buffer
#[derive(Debug)]
pub struct DebugWriter {
    ptr: *mut u8,
    ofs: usize,
}

impl DebugWriter {
    /// Creates a new `DebugWriter`
    pub fn new() -> Self {
        DebugWriter {
            ptr: unsafe { DEBUG_BUFFER.as_mut_ptr() },
            ofs: 0,
        }
    }

    pub fn ptr(&self) -> *mut u8 {
        self.ptr
    }

    pub fn ofs(&self) -> usize {
        self.ofs
    }

    fn slice(&mut self) -> Option<&mut [u8]> {
        // if there is no space left, return `None`
        if self.ofs < DEBUG_BUFFER_SIZE {
            unsafe {
                Some(&mut *slice_from_raw_parts_mut(
                    self.ptr.add(self.ofs),
                    DEBUG_BUFFER_SIZE - self.ofs,
                ))
            }
        } else {
            None
        }
    }
}

impl Default for DebugWriter {
    fn default() -> Self {
        DebugWriter::new()
    }
}

impl fmt::Write for DebugWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let bytes = s.as_bytes();

        let slice = self.slice().ok_or(fmt::Error)?;
        slice[..bytes.len()].copy_from_slice(bytes);

        self.ofs += bytes.len();

        Ok(())
    }
}

/// Macro to format and send debug output to the host
#[macro_export]
macro_rules! debug {
    ($($tt:tt)*) => {
        #[allow(unused)]
        use core::fmt::Write as _;

        let mut w = $crate::debug::DebugWriter::new();
        write!(&mut w, $($tt)*).unwrap();

        unsafe {
            $crate::debug::ext::host_debug(w.ptr() as i32, w.ofs() as u32)
        }
    };
}
