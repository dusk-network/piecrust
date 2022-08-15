// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use crate::state::with_arg_buf;
use crate::SCRATCH_BUF_BYTES;

use rkyv::ser::serializers::{
    BufferScratch, BufferSerializer, CompositeSerializer,
};
use rkyv::ser::Serializer;
use rkyv::{check_archived_root, Archive, Deserialize, Serialize};

use crate::types::{StandardBufSerializer, StandardDeserialize};

/// Wrap a query with its respective (de)serializers.
///
/// Returns the length of result written to the buffer.
pub fn wrap_query<A, R, F>(arg_len: u32, f: F) -> u32
where
    A: Archive,
    A::Archived: StandardDeserialize<A>,
    R: for<'a> Serialize<StandardBufSerializer<'a>>,
    F: Fn(A) -> R,
{
    with_arg_buf(|buf| {
        let slice = &buf[..arg_len as usize];
        let aa: &A::Archived = check_archived_root::<A>(slice).unwrap();
        let a: A = aa.deserialize(&mut rkyv::Infallible).unwrap();
        let ret = f(a);

        let mut sbuf = [0u8; SCRATCH_BUF_BYTES];
        let scratch = BufferScratch::new(&mut sbuf);
        let ser = BufferSerializer::new(buf);
        let mut composite =
            CompositeSerializer::new(ser, scratch, rkyv::Infallible);
        composite.serialize_value(&ret).expect("infallible");
        composite.pos() as u32
    })
}

/// Wrap a transaction with its respective (de)serializers.
///
/// Returns the length of result written to the buffer.
pub fn wrap_transaction<A, R, F>(arg_len: u32, mut f: F) -> u32
where
    A: Archive,
    A::Archived: StandardDeserialize<A>,
    R: for<'a> Serialize<StandardBufSerializer<'a>>,
    F: FnMut(A) -> R,
{
    with_arg_buf(|buf| {
        let slice = &buf[..arg_len as usize];
        let aa: &A::Archived = check_archived_root::<A>(slice).unwrap();
        let a: A = aa.deserialize(&mut rkyv::Infallible).unwrap();
        let ret = f(a);

        let mut sbuf = [0u8; SCRATCH_BUF_BYTES];
        let scratch = BufferScratch::new(&mut sbuf);
        let ser = BufferSerializer::new(buf);
        let mut composite =
            CompositeSerializer::new(ser, scratch, rkyv::Infallible);
        composite.serialize_value(&ret).expect("infallible");
        composite.pos() as u32
    })
}
