// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Contract to create a `Box` to some given data, to change and read that
//! data.

#![no_std]

extern crate alloc;

use alloc::boxed::Box;

use piecrust_macros::contract;

/// Struct that describes the state of the box contract
// One Box, many `Boxen`
pub struct Boxen {
    a: Option<Box<i16>>,
}

/// State of the box contract
static mut STATE: Boxen = Boxen { a: None };

#[contract]
impl Boxen {
    /// Set the data pointed to by the `Box`, or create a new `Box` if it
    /// doesn't exist
    pub fn set(&mut self, x: i16) {
        match self.a.as_mut() {
            Some(o) => **o = x,
            None => self.a = Some(Box::new(x)),
        }
    }

    /// Return the boxed data
    pub fn get(&self) -> Option<i16> {
        self.a.as_ref().map(|i| **i)
    }
}
