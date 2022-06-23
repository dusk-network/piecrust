// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.
#![no_std]
#![no_main]

use rkyv::ser::serializers::BufferSerializer;
use rkyv::ser::Serializer;
use rkyv::{archived_value, Deserialize, Infallible};

use dallo::Box;

// One Box, many `Boxen`
pub struct Boxen {
    a: Option<Box<i16>>,
}

const ARGBUF_LEN: usize = 6;

#[no_mangle]
static mut A: [u8; ARGBUF_LEN] = [0u8; ARGBUF_LEN];
#[no_mangle]
static AL: i32 = ARGBUF_LEN as i32;

static mut SELF: Boxen = Boxen { a: None };

impl Boxen {
    pub fn set(&mut self, x: i16) {
        match self.a.as_mut() {
            Some(o) => **o = x,
            None => self.a = Some(Box::new(x)),
        }
    }

    pub fn get(&mut self) -> Option<i16> {
        self.a.as_ref().map(|i| **i)
    }
}

#[no_mangle]
fn set(a: i32) -> i32 {
    let i = unsafe { archived_value::<i16>(&A, a as usize) };
    let i = i.deserialize(&mut Infallible).unwrap();
    unsafe { SELF.set(i) };
    0
}

#[no_mangle]
fn get(_: i32) -> i32 {
    let ret = unsafe { SELF.get() };
    let mut ser = unsafe { BufferSerializer::new(&mut A) };
    ser.serialize_value(&ret).unwrap() as i32
}
