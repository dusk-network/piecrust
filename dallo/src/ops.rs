// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use crate::ModuleId;

mod ext {
    use crate::MODULE_ID_BYTES;

    extern "C" {
        pub static SELF_ID: [u8; MODULE_ID_BYTES];
    }
}

pub fn self_id() -> &'static ModuleId {
    unsafe {
        let callee_ptr = ext::SELF_ID.as_ptr();
        let callee = callee_ptr as *const ModuleId;
        &*callee
    }
}
