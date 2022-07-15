// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

#[derive(Debug)]
pub struct MemHandler {
    heap_base: usize,
}

impl MemHandler {
    pub fn new(heap_base: usize) -> Self {
        MemHandler { heap_base }
    }

    pub fn align_to(&mut self, n: usize) {
        self.heap_base += self.heap_base % n;
    }

    pub fn alloc(&mut self, size: usize, align: usize) -> usize {
        self.align_to(align);
        let ofs = self.heap_base;
        self.heap_base += size;
        println!("allocating {} bytes at {:08x}", size, ofs);
        ofs
    }
}
