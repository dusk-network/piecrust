// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use core::fmt::{self, Write};

use crate::abi::state;
use crate::ARGBUF_LEN;

extern "C" {
    pub fn hdebug(arg_len: u32);
}

/// A small struct that can `fmt::Write` to the argument buffer.
///
/// It is just an offset to the argument buffer, representing how much has been
/// written to it.
#[derive(Default)]
pub struct ArgbufWriter(usize);

impl ArgbufWriter {
    pub fn ofs(&self) -> usize {
        self.0
    }
}

impl Write for ArgbufWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let bytes = s.as_bytes();
        let bytes_len = bytes.len();

        let new_ofs = self.0 + bytes_len;

        if new_ofs > ARGBUF_LEN {
            return Err(fmt::Error);
        }

        state::with_arg_buf(|buf| {
            buf[self.0..new_ofs].copy_from_slice(bytes);
        });

        self.0 = new_ofs;

        Ok(())
    }
}

/// Macro to format and send debug output to the host
#[macro_export]
macro_rules! debug {
    ($($tt:tt)*) => {
        #[allow(unused)]
        use core::fmt::Write as _;

        let mut w = $crate::ArgbufWriter::default();
        write!(&mut w, $($tt)*).unwrap();

        unsafe { $crate::hdebug(w.ofs() as u32) };
    };
}
