// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use rkyv::ser::serializers::{
    BufferScratch, BufferSerializer, CompositeSerializer,
};
use rkyv::ser::Serializer;
use rkyv::{archived_value, Archive, Deserialize, Serialize};

pub type Ser<'a> = CompositeSerializer<
    BufferSerializer<&'a mut [u8]>,
    BufferScratch<&'a mut [u8; 16]>,
>;

pub fn wrap_query<A, R, F>(buf: &mut [u8], arg_ofs: i32, f: F) -> i32
where
    A: Archive,
    A::Archived: Deserialize<A, rkyv::Infallible>,
    R: for<'a> Serialize<Ser<'a>>,
    F: Fn(A) -> R,
{
    let aa: &A::Archived =
        unsafe { archived_value::<A>(buf, arg_ofs as usize) };
    let a: A = aa.deserialize(&mut rkyv::Infallible).unwrap();
    let ret = f(a);

    let mut sbuf = [0u8; 16];
    let scratch = BufferScratch::new(&mut sbuf);
    let ser = BufferSerializer::new(buf);
    let mut composite =
        CompositeSerializer::new(ser, scratch, rkyv::Infallible);

    composite.serialize_value(&ret).unwrap() as i32
}

pub fn wrap_transaction<A, R, F>(buf: &mut [u8], arg_ofs: i32, f: F) -> i32
where
    A: Archive,
    A::Archived: Deserialize<A, rkyv::Infallible>,
    R: for<'a> Serialize<Ser<'a>>,
    F: FnOnce(A) -> R,
{
    let aa: &A::Archived =
        unsafe { archived_value::<A>(buf, arg_ofs as usize) };
    let a: A = aa.deserialize(&mut rkyv::Infallible).unwrap();
    let ret = f(a);

    let mut sbuf = [0u8; 16];
    let scratch = BufferScratch::new(&mut sbuf);
    let ser = BufferSerializer::new(buf);
    let mut composite =
        CompositeSerializer::new(ser, scratch, rkyv::Infallible);
    composite.serialize_value(&ret).unwrap() as i32
}
