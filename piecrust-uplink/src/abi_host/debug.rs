// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use core::fmt::Arguments;

use super::{externs, slice_len};

/// Formats and sends a debug message to the host.
pub fn hdebug(arguments: Arguments<'_>) {
    let message = alloc::fmt::format(arguments);
    unsafe {
        externs::hdebug_v2(
            message.as_ptr(),
            slice_len(message.as_bytes(), "debug message length"),
        )
    }
}

/// Formats and sends debug output to the host.
#[cfg(not(feature = "abi"))]
#[macro_export]
macro_rules! debug {
    ($($tt:tt)*) => {
        $crate::hdebug(format_args!($($tt)*));
    };
}
