// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use core::panic::PanicInfo;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    #[cfg(feature = "debug")]
    if let Some(msg) = _info.message() {
        crate::debug!("{msg}");
    }
    unreachable!()
}

#[lang = "eh_personality"]
extern "C" fn eh_personality() {}
