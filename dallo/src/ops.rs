// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use crate::ModuleId;

mod ext {
    use crate::MODULE_ID_BYTES;

    extern "C" {
        pub static CALLER: [u8; MODULE_ID_BYTES + 1];
        pub static CALLEE: [u8; MODULE_ID_BYTES];
    }
}

pub fn caller() -> Option<&'static ModuleId> {
    unsafe {
        match ext::CALLER[0] == 0 {
            true => None,
            false => {
                let caller_ptr = ext::CALLER[1..].as_ptr();
                let caller = caller_ptr as *const ModuleId;
                Some(&*caller)
            }
        }
    }
}

pub fn callee() -> &'static ModuleId {
    unsafe {
        let callee_ptr = ext::CALLEE.as_ptr();
        let callee = callee_ptr as *const ModuleId;
        &*callee
    }
}
