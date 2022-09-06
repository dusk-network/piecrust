// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

extern "C" {
    pub fn host_debug(ofs: i32, len: u32);
}

pub const DEBUG_BUFFER_SIZE: usize = 64 * 1024;
pub static mut DEBUG_BUFFER: [u8; DEBUG_BUFFER_SIZE] = [0u8; DEBUG_BUFFER_SIZE];

/// Macro to format and send debug output to the host
#[macro_export]
macro_rules! debug {
	($($tt:tt)*) => {
        #[allow(unused)]
        use core::fmt::Write as _;

        let buf = unsafe {&mut $crate::debug::DEBUG_BUFFER };

        let len = {
		    let mut w = $crate::bufwriter::BufWriter::new(buf);
		    write!(&mut w, $($tt)*).unwrap();
            w.ofs() as u32
        };
        let ptr = buf.as_ptr() as i32;

        unsafe { $crate::debug::host_debug(ptr, len) }
	};

}
