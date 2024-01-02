// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Contract that calculates the nth number of the Fibonacci sequence

#![no_std]

use piecrust_macros::contract;

/// Struct that describes the state of the fibonacci contract
pub struct Fibonacci;

#[allow(unused)]
/// State of the fibonacci contract
static mut STATE: Fibonacci = Fibonacci;

#[contract]
impl Fibonacci {
    /// Calculate the nth number in the fibonacci sequence
    pub fn nth(n: u32) -> u64 {
        match n {
            0 | 1 => 1,
            n => Self::nth(n - 1) + Self::nth(n - 2),
        }
    }
}
