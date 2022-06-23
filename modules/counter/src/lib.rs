// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.
#![no_std]
#![no_main]

use rkyv::ser::serializers::BufferSerializer;
use rkyv::ser::Serializer;
use rkyv::{archived_value, Deserialize};

#[derive(Default)]
pub struct Counter {
    value: i32,
}

#[allow(unused)]
use dallo;

const ARGBUF_LEN: usize = 4;

#[no_mangle]
static mut A: [u8; ARGBUF_LEN] = [0u8; ARGBUF_LEN];
#[no_mangle]
static AL: i32 = ARGBUF_LEN as i32;

static mut SELF: Counter = Counter { value: 0xfc };

impl Counter {
    pub fn read_value(&self) -> i32 {
        self.value.into()
    }

    pub fn increment(&mut self) {
        self.value += 1;
    }

    pub fn mogrify(&mut self, x: i32) {
        let x: i32 = x.into();
        self.value -= x;
    }
}

#[no_mangle]
fn read_value(_: i32) -> i32 {
    let ret = unsafe { SELF.read_value() };
    let mut ser = unsafe { BufferSerializer::new(&mut A) };
    ser.serialize_value(&ret).unwrap() as i32
}

#[no_mangle]
fn increment(_: i32) -> i32 {
    unsafe { SELF.increment() }
    0
}

#[no_mangle]
fn mogrify(arg: i32) -> i32 {
    let ret = {
        let i = unsafe { archived_value::<i32>(&A, arg as usize) };
        let i: i32 = i.deserialize(&mut rkyv::Infallible).unwrap();
        unsafe { SELF.mogrify(i) }
    };

    let mut ser = unsafe { BufferSerializer::new(&mut A) };
    ser.serialize_value(&ret).unwrap() as i32
}
