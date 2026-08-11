// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use rkyv::validation::validators::DefaultValidator;
use rkyv::{
    Archive, Deserialize, Infallible, Serialize, archived_root,
    check_archived_root,
};

use super::{HostBufSerializer, publish_call_output, read_call_input};
use crate::SCRATCH_BUF_BYTES;

/// Wraps a contract call with checked `rkyv` deserialization and dynamic
/// serialization.
///
/// `arg_len` is retained for the exported `(u32) -> u32` contract signature.
/// The authoritative input length is obtained from the host.
pub fn wrap_call<A, R, F>(_arg_len: u32, f: F) -> u32
where
    A: Archive,
    A::Archived: Deserialize<A, Infallible>
        + for<'b> bytecheck::CheckBytes<DefaultValidator<'b>>,
    R: Serialize<HostBufSerializer>,
    F: Fn(A) -> R,
{
    let input = read_call_input();
    let archived = check_archived_root::<A>(&input)
        .expect("argument should correctly deserialize");
    let argument = archived.deserialize(&mut Infallible).unwrap();

    let output = rkyv::to_bytes::<_, SCRATCH_BUF_BYTES>(&f(argument))
        .expect("infallible");
    publish_call_output(&output)
}

/// Wraps a contract call with unchecked `rkyv` deserialization and dynamic
/// serialization.
///
/// This function assumes the host input is a valid archive of `A`. Passing
/// malformed or untrusted bytes can cause undefined behavior. Prefer
/// [`wrap_call`] unless the caller input is fully trusted.
pub fn wrap_call_unchecked<A, R, F>(_arg_len: u32, f: F) -> u32
where
    A: Archive,
    A::Archived: Deserialize<A, Infallible>,
    R: Serialize<HostBufSerializer>,
    F: Fn(A) -> R,
{
    let input = read_call_input();
    let archived = unsafe { archived_root::<A>(&input) };
    let argument = archived.deserialize(&mut Infallible).unwrap();

    let output = rkyv::to_bytes::<_, SCRATCH_BUF_BYTES>(&f(argument))
        .expect("infallible");
    publish_call_output(&output)
}
