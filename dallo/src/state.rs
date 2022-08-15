// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use rkyv::{
    check_archived_root,
    ser::serializers::{BufferScratch, BufferSerializer, CompositeSerializer},
    ser::Serializer,
    Archive, Deserialize, Infallible, Serialize,
};

use crate::{
    RawQuery, RawResult, RawTransaction, StandardBufSerializer,
    StandardDeserialize, SCRATCH_BUF_BYTES,
};

mod arg_buf {
    use crate::ARGBUF_LEN;

    #[no_mangle]
    static mut A: [u64; ARGBUF_LEN / 8] = [0; ARGBUF_LEN / 8];

    pub fn with_arg_buf<F, R>(f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let buf = unsafe { &mut A };
        let first = &mut buf[0];
        let slice = unsafe {
            let first_byte: &mut u8 = core::mem::transmute(first);
            core::slice::from_raw_parts_mut(first_byte, ARGBUF_LEN)
        };

        f(slice)
    }
}

pub(crate) use arg_buf::with_arg_buf;

mod ext {
    extern "C" {
        pub(crate) fn q(
            mod_id: *const u8,
            name: *const u8,
            name_len: u32,
            arg_len: u32,
        ) -> u32;
        pub(crate) fn t(
            mod_id: *const u8,
            name: *const u8,
            name_len: u32,
            arg_len: u32,
        ) -> u32;

        pub(crate) fn height() -> u32;
        pub(crate) fn caller() -> u32;
        pub(crate) fn emit(arg_len: u32);
        pub(crate) fn spent() -> u32;
    }
}

fn extern_query(module_id: ModuleId, name: &str, arg_len: u32) -> u32 {
    let mod_ptr = module_id.as_ptr();
    let name_ptr = name.as_ptr();
    let name_len = name.as_bytes().len() as u32;
    unsafe { ext::q(mod_ptr, name_ptr, name_len, arg_len) }
}

fn extern_transaction(module_id: ModuleId, name: &str, arg_len: u32) -> u32 {
    let mod_ptr = module_id.as_ptr();
    let name_ptr = name.as_ptr();
    let name_len = name.as_bytes().len() as u32;
    unsafe { ext::t(mod_ptr, name_ptr, name_len, arg_len) }
}

use crate::ModuleId;
use core::ops::{Deref, DerefMut};

pub struct State<S> {
    inner: S,
}

impl<S> State<S> {
    pub const fn new(inner: S) -> Self {
        State { inner }
    }
}

impl<S> Deref for State<S> {
    type Target = S;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<S> DerefMut for State<S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

pub fn query<Arg, Ret>(mod_id: ModuleId, name: &str, arg: Arg) -> Ret
where
    Arg: for<'a> Serialize<StandardBufSerializer<'a>>,
    Ret: Archive,
    Ret::Archived: StandardDeserialize<Ret>,
{
    let arg_len = with_arg_buf(|buf| {
        let mut sbuf = [0u8; SCRATCH_BUF_BYTES];
        let scratch = BufferScratch::new(&mut sbuf);
        let ser = BufferSerializer::new(buf);
        let mut composite =
            CompositeSerializer::new(ser, scratch, rkyv::Infallible);

        composite.serialize_value(&arg).expect("infallible");
        composite.pos() as u32
    });

    let ret_len = extern_query(mod_id, name, arg_len);

    with_arg_buf(|buf| {
        let slice = &buf[..ret_len as usize];
        let ret = check_archived_root::<Ret>(slice).unwrap();
        ret.deserialize(&mut Infallible).expect("Infallible")
    })
}

pub fn query_raw(mod_id: ModuleId, raw: RawQuery) -> RawResult {
    with_arg_buf(|buf| {
        let bytes = raw.arg_bytes();
        buf[..bytes.len()].copy_from_slice(bytes);
    });

    let name = raw.name();
    let arg_len = raw.arg_bytes().len() as u32;
    let ret_len = extern_query(mod_id, name, arg_len);

    with_arg_buf(|buf| RawResult::new(&buf[..ret_len as usize]))
}

/// Return the current height.
pub fn height() -> u64 {
    with_arg_buf(|buf| {
        let ret_len = unsafe { ext::height() };

        let ret = check_archived_root::<u64>(&buf[..ret_len as usize]).unwrap();
        ret.deserialize(&mut Infallible).expect("Infallible")
    })
}

/// Return the ID of the calling module. The returned id will be
/// uninitialized if there is no caller - meaning this is the first module
/// to be called.
pub fn caller() -> ModuleId {
    with_arg_buf(|buf| {
        let ret_len = unsafe { ext::caller() };
        let ret =
            check_archived_root::<ModuleId>(&buf[..ret_len as usize]).unwrap();
        ret.deserialize(&mut Infallible).expect("Infallible")
    })
}

/// Emits an event with the given data.
pub fn emit<D>(data: D)
where
    for<'a> D: Serialize<StandardBufSerializer<'a>>,
{
    with_arg_buf(|buf| {
        let mut sbuf = [0u8; SCRATCH_BUF_BYTES];
        let scratch = BufferScratch::new(&mut sbuf);
        let ser = BufferSerializer::new(buf);
        let mut composite =
            CompositeSerializer::new(ser, scratch, rkyv::Infallible);

        composite.serialize_value(&data).unwrap();
        let arg_len = composite.pos() as u32;

        unsafe { ext::emit(arg_len) }
    });
}

pub fn spent() -> u64 {
    with_arg_buf(|buf| {
        let ret_len = unsafe { ext::spent() };

        let ret = check_archived_root::<u64>(&buf[..ret_len as usize]).unwrap();
        ret.deserialize(&mut Infallible).expect("Infallible")
    })
}

impl<S> State<S> {
    pub fn transact_raw(
        &self,
        mod_id: ModuleId,
        raw: RawTransaction,
    ) -> RawResult {
        with_arg_buf(|buf| {
            let bytes = raw.arg_bytes();
            buf[..bytes.len()].copy_from_slice(bytes);
        });

        let name = raw.name();
        let arg_len = raw.arg_bytes().len() as u32;
        let ret_len = extern_query(mod_id, name, arg_len);

        with_arg_buf(|buf| RawResult::new(&buf[..ret_len as usize]))
    }

    pub fn transact<Arg, Ret>(
        &mut self,
        mod_id: ModuleId,
        name: &str,
        arg: Arg,
    ) -> Ret
    where
        Arg: for<'a> Serialize<StandardBufSerializer<'a>>,
        Ret: Archive,
        Ret::Archived: StandardDeserialize<Ret>,
    {
        let arg_len = with_arg_buf(|buf| {
            let mut sbuf = [0u8; SCRATCH_BUF_BYTES];
            let scratch = BufferScratch::new(&mut sbuf);
            let ser = BufferSerializer::new(buf);
            let mut composite =
                CompositeSerializer::new(ser, scratch, rkyv::Infallible);

            composite.serialize_value(&arg).unwrap();
            composite.pos() as u32
        });

        let ret_len = extern_transaction(mod_id, name, arg_len);

        with_arg_buf(|buf| {
            let slice = &buf[..ret_len as usize];
            let ret = check_archived_root::<Ret>(slice)
                .expect("invalid return value");
            ret.deserialize(&mut Infallible).expect("Infallible")
        })
    }
}
