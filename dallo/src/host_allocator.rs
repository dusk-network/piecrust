// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use alloc::alloc::{GlobalAlloc, Layout};

pub struct HostAlloc;

extern "C" {
    fn alloc(size: usize, align: usize) -> *mut u8;
    fn dealloc(ptr: *mut u8);
}

unsafe impl GlobalAlloc for HostAlloc {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        alloc(l.size(), l.align())
    }

    unsafe fn dealloc(&self, m: *mut u8, _: Layout) {
        dealloc(m)
    }
}
