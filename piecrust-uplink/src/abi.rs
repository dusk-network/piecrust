// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use core::fmt::{self, Write};

mod allocator;

mod handlers;

mod helpers;
pub use helpers::*;

mod state;
pub use state::*;

#[cfg(feature = "debug")]
#[cfg_attr(docsrs, doc(cfg(feature = "debug")))]
mod debug;
#[cfg(feature = "debug")]
pub use debug::*;

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

        if new_ofs > crate::ARGBUF_LEN {
            return Err(fmt::Error);
        }

        state::with_arg_buf(|buf, _| {
            buf[self.0..new_ofs].copy_from_slice(bytes);
        });

        self.0 = new_ofs;

        Ok(())
    }
}
