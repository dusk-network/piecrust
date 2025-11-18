// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use crate::abi::state::with_arg_buf;
use crate::SCRATCH_BUF_BYTES;

use rkyv::ser::serializers::{
    BufferScratch, BufferSerializer, CompositeSerializer,
};
use rkyv::ser::Serializer;
use rkyv::validation::validators::DefaultValidator;
use rkyv::{
    archived_root, check_archived_root, Archive, Deserialize, Infallible,
    Serialize,
};

use crate::types::StandardBufSerializer;

/// Wrap a call with its respective (de)serializers.
/// Checks integrity of the arguments.
///
/// Returns the length of result written to the buffer.
pub fn wrap_call<A, R, F>(arg_len: u32, f: F) -> u32
where
    A: Archive,
    A::Archived: Deserialize<A, Infallible>
        + for<'b> bytecheck::CheckBytes<DefaultValidator<'b>>,
    R: for<'a> Serialize<StandardBufSerializer<'a>>,
    F: Fn(A) -> R,
{
    println!("in wrap_call");
    with_arg_buf(|buf| {
        let slice = &buf[..arg_len as usize];

        let aa: &A::Archived = check_archived_root::<A>(slice)
            .expect("Argument should correctly deserialize");
        let a: A = aa.deserialize(&mut Infallible).unwrap();

        let ret = f(a);

        let mut sbuf = [0u8; SCRATCH_BUF_BYTES];
        let scratch = BufferScratch::new(&mut sbuf);
        let ser = BufferSerializer::new(buf);
        let mut composite = CompositeSerializer::new(ser, scratch, Infallible);
        composite.serialize_value(&ret).expect("infallible");
        composite.pos() as u32
    })
}

/// Wrap a call with its respective (de)serializers.
/// Does not check the integrity of arguments.
///
/// Returns the length of result written to the buffer.
pub fn wrap_call_unchecked<A, R, F>(arg_len: u32, f: F) -> u32
where
    A: Archive,
    A::Archived: Deserialize<A, Infallible>,
    R: for<'a> Serialize<StandardBufSerializer<'a>>,
    F: Fn(A) -> R,
{
    println!("in wrap_call_unchecked");
    with_arg_buf(|buf| {
        let slice = &buf[..arg_len as usize];

        let aa: &A::Archived = unsafe { archived_root::<A>(slice) };
        let a: A = aa.deserialize(&mut Infallible).unwrap();

        let ret = f(a);

        let mut sbuf = [0u8; SCRATCH_BUF_BYTES];
        let scratch = BufferScratch::new(&mut sbuf);
        let ser = BufferSerializer::new(buf);
        let mut composite = CompositeSerializer::new(ser, scratch, Infallible);
        composite.serialize_value(&ret).expect("infallible");
        composite.pos() as u32
    })
}
