// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

extern "C" {
    pub fn hdebug(arg_len: u32);
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
