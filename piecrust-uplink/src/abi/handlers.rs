// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use core::fmt::Write;
use core::panic::PanicInfo;

extern "C" {
    pub fn panic(arg_len: u32);
}

#[panic_handler]
unsafe fn handle_panic(info: &PanicInfo) -> ! {
    let mut w = crate::ArgbufWriter::default();

    // If we fail in writing to the argument buffer, we just call `panic` after
    // writing a standard message instead.

    if w.write_fmt(format_args!("{}", info.message())).is_err() {
        w = crate::ArgbufWriter::default();
        let _ = write!(w, "PANIC INFO TOO LONG");
    }

    panic(w.ofs() as u32);
    unreachable!()
}

#[lang = "eh_personality"]
extern "C" fn eh_personality() {}
