// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::ptr;

// Define the argument buffer used to communicate between contract and host
#[no_mangle]
pub static mut A: [u8; 65536] = [0; 65536];

// Define second argument buffer for the Economic Protocol
#[no_mangle]
pub static mut ECO_MODE: [u8; 16] = [0; 16];

// ==== Host functions ====
//
// These functions are provided by the host. See `piecrust-uplink` for a full
// list. Here we only declare the ones we will need.
mod ext {
    extern "C" {
        pub fn c(
            contract_id: *const u8,
            fn_name: *const u8,
            fn_name_len: u32,
            fn_arg_len: u32,
            gas_limit: u64,
        ) -> i32;
        pub fn hd(name: *const u8, name_len: u32) -> u32;
    }
}

// ==== Helper functions ====
//
// These will help us write the exported functions underneath

// Reads a contract ID from the argument buffer
unsafe fn read_contract_id(id: *mut u8) {
    ptr::copy(&A[0], id, 32);
}

// Calls the counter contract to increment the counter
unsafe fn increment_counter(contract_id: *const u8) {
    let fn_name = b"increment";
    ext::c(contract_id, fn_name.as_ptr(), fn_name.len() as u32, 0, 0);
}

// Reads a 64-bit from the argument buffer
unsafe fn read_integer(i: *mut i64) {
    ptr::copy(&A[0], i.cast(), 8);
}

// Writes a 64-bit integer to the argument buffer
unsafe fn write_integer(i: i64) {
    let i: *const i64 = &i;
    let i: *const u8 = i as _;
    ptr::copy(&*i, &mut A[0], 8);
}

// Calls the counter contract to read the counter
unsafe fn read_counter(contract_id: *const u8) -> i64 {
    let fn_name = b"read_value";
    ext::c(contract_id, fn_name.as_ptr(), fn_name.len() as u32, 0, 0);

    let mut i = 0;
    read_integer(&mut i);
    i
}

// ==== Exported functions ====

// Increments and reads the counter contract. The function expects the counter
// contract ID to be written to the argument buffer before being called.
#[no_mangle]
unsafe fn increment_and_read(_: i32) -> i32 {
    let mut counter_id = [0u8; 32];

    read_contract_id(&mut counter_id[0]);
    increment_counter(&counter_id[0]);

    write_integer(read_counter(&counter_id[0]));

    8
}

// Calls the "hd" extern with an (almost) certainly out of bounds pointer, in an
// effort to trigger an error.
#[no_mangle]
unsafe fn out_of_bounds(_: i32) -> i32 {
    ext::hd(4398046511103 as *const u8, 2);
    0
}
