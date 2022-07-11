// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.
#![no_std]
#![no_main]

#[global_allocator]
static ALLOCATOR: dallo::HostAlloc = dallo::HostAlloc;

#[derive(Default)]
pub struct Callcenter;

use dallo::*;

const ARGBUF_LEN: usize = 1024;

#[no_mangle]
static mut A: [u8; ARGBUF_LEN] = [0u8; ARGBUF_LEN];
#[no_mangle]
static AL: i32 = ARGBUF_LEN as i32;

static mut SELF: Callcenter = Callcenter;

impl Callcenter {
    pub fn delegate_query(&self, _id: ModuleId, _raw: RawQuery) -> ReturnBuf {
        todo!()
    }

    pub fn delegate_transaction(
        &self,
        _id: ModuleId,
        _raw: RawTransaction,
    ) -> ReturnBuf {
        todo!()
    }
}

#[no_mangle]
unsafe fn delegate_query(a: i32) -> i32 {
    dallo::wrap_query(&mut A, a, |(mod_id, raw)| {
        SELF.delegate_query(mod_id, raw)
    })
}

#[no_mangle]
unsafe fn delegate_transaction(a: i32) -> i32 {
    dallo::wrap_transaction(&mut A, a, |(mod_id, raw)| {
        SELF.delegate_transaction(mod_id, raw)
    })
}
