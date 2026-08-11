// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use alloc::fmt::format;
use core::panic::PanicInfo;

use super::{externs, slice_len};

#[panic_handler]
fn handle_panic(info: &PanicInfo) -> ! {
    let message = format(format_args!("{}", info.message()));
    unsafe {
        externs::panic_v2(
            message.as_ptr(),
            slice_len(message.as_bytes(), "panic message length"),
        )
    }
}
