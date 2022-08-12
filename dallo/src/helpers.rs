// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use crate::SCRATCH_BUF_BYTES;

use rkyv::ser::serializers::{
    BufferScratch, BufferSerializer, CompositeSerializer,
};
use rkyv::ser::Serializer;
use rkyv::{archived_root, Archive, Deserialize, Serialize};

pub type StandardBufSerializer<'a> = CompositeSerializer<
    BufferSerializer<&'a mut [u8]>,
    BufferScratch<&'a mut [u8; SCRATCH_BUF_BYTES]>,
>;

/// Wrap a query with its respective (de)serializers.
///
/// Returns the length of result written to the buffer.
pub fn wrap_query<A, R, F>(buf: &mut [u8], arg_len: u32, f: F) -> u32
where
    A: Archive,
    A::Archived: Deserialize<A, rkyv::Infallible>,
    R: for<'a> Serialize<StandardBufSerializer<'a>>,
    F: Fn(A) -> R,
{
    let slice = &buf[..arg_len as usize];
    let aa: &A::Archived = unsafe { archived_root::<A>(slice) };
    let a: A = aa.deserialize(&mut rkyv::Infallible).unwrap();
    let ret = f(a);

    let mut sbuf = [0u8; SCRATCH_BUF_BYTES];
    let scratch = BufferScratch::new(&mut sbuf);
    let ser = BufferSerializer::new(buf);
    let mut composite =
        CompositeSerializer::new(ser, scratch, rkyv::Infallible);
    composite.serialize_value(&ret).expect("infallible");
    composite.pos() as u32
}

/// Wrap a transaction with its respective (de)serializers.
///
/// Returns the length of result written to the buffer.
pub fn wrap_transaction<A, R, F>(buf: &mut [u8], arg_len: u32, f: F) -> u32
where
    A: Archive,
    A::Archived: Deserialize<A, rkyv::Infallible>,
    R: for<'a> Serialize<StandardBufSerializer<'a>>,
    F: FnOnce(A) -> R,
{
    let slice = &buf[..arg_len as usize];
    let aa: &A::Archived = unsafe { archived_root::<A>(slice) };
    let a: A = aa.deserialize(&mut rkyv::Infallible).unwrap();
    let ret = f(a);

    let mut sbuf = [0u8; SCRATCH_BUF_BYTES];
    let scratch = BufferScratch::new(&mut sbuf);
    let ser = BufferSerializer::new(buf);
    let mut composite =
        CompositeSerializer::new(ser, scratch, rkyv::Infallible);
    composite.serialize_value(&ret).expect("infallible");
    composite.pos() as u32
}
