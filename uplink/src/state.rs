// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use rkyv::{
    archived_root,
    ser::serializers::{BufferScratch, BufferSerializer, CompositeSerializer},
    ser::Serializer,
    Archive, Deserialize, Infallible, Serialize,
};

use crate::{
    RawQuery, RawResult, RawTransaction, StandardBufSerializer,
    SCRATCH_BUF_BYTES,
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
            mod_id_ofs: *const u8,
            name: *const u8,
            name_len: u32,
            arg_len: u32,
        ) -> u32;
        #[allow(unused)]
        pub(crate) fn nq(name: *const u8, name_len: u32, arg_len: u32) -> u32;
        pub(crate) fn t(
            mod_id_ofs: *const u8,
            name: *const u8,
            name_len: u32,
            arg_len: u32,
        ) -> u32;

        pub(crate) fn height() -> u32;
        pub(crate) fn caller() -> u32;
        pub(crate) fn emit(arg_len: u32);
        pub(crate) fn limit() -> u32;
        pub(crate) fn spent() -> u32;
    }
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
    Ret::Archived: Deserialize<Ret, Infallible>,
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

    let name_slice = name.as_bytes();

    let ret_len = unsafe {
        ext::q(
            &mod_id.as_bytes()[0],
            &name_slice[0],
            name_slice.len() as u32,
            arg_len,
        )
    };

    with_arg_buf(|buf| {
        let slice = &buf[..ret_len as usize];
        let ret = unsafe { archived_root::<Ret>(slice) };
        ret.deserialize(&mut Infallible).expect("Infallible")
    })
}

pub fn query_raw(mod_id: ModuleId, raw: RawQuery) -> RawResult {
    with_arg_buf(|buf| {
        let bytes = raw.arg_bytes();
        buf[..bytes.len()].copy_from_slice(bytes);
    });

    let name = raw.name_bytes();
    let arg_len = raw.arg_bytes().len() as u32;

    let ret_len = unsafe {
        crate::debug!("Corv");

        ext::q(&mod_id.as_bytes()[0], &name[0], name.len() as u32, arg_len)
    };

    crate::debug!("D");

    with_arg_buf(|buf| RawResult::new(&buf[..ret_len as usize]))
}

pub fn native_query<Arg, Ret>(_name: &str, arg: Arg) -> Ret
where
    Arg: for<'a> Serialize<StandardBufSerializer<'a>>,
    Ret: Archive,
    Ret::Archived: Deserialize<Ret, Infallible>,
{
    let _arg_len = with_arg_buf(|buf| {
        let mut sbuf = [0u8; SCRATCH_BUF_BYTES];
        let scratch = BufferScratch::new(&mut sbuf);
        let ser = BufferSerializer::new(buf);
        let mut composite =
            CompositeSerializer::new(ser, scratch, rkyv::Infallible);

        composite.serialize_value(&arg).expect("infallible");
        composite.pos() as u32
    });

    // let _ret_len: u32 = todo!();

    // with_arg_buf(|buf| {
    //     let slice = &buf[..ret_len as usize];
    //     let ret = unsafe { archived_root::<Ret>(slice) };
    //     ret.deserialize(&mut Infallible).expect("Infallible")
    // })
    todo!()
}

/// Return the current height.
pub fn height() -> u64 {
    with_arg_buf(|buf| {
        let ret_len = unsafe { ext::height() };

        let ret = unsafe { archived_root::<u64>(&buf[..ret_len as usize]) };
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
            unsafe { archived_root::<ModuleId>(&buf[..ret_len as usize]) };
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

pub fn limit() -> u64 {
    with_arg_buf(|buf| {
        let ret_len = unsafe { ext::limit() };
        let ret = unsafe { archived_root::<u64>(&buf[..ret_len as usize]) };
        ret.deserialize(&mut Infallible).expect("Infallible")
    })
}

pub fn spent() -> u64 {
    with_arg_buf(|buf| {
        let ret_len = unsafe { ext::spent() };
        let ret = unsafe { archived_root::<u64>(&buf[..ret_len as usize]) };
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

        let name = raw.name_bytes();
        let arg_len = raw.arg_bytes().len() as u32;

        // ERROR?
        let ret_len = unsafe {
            ext::t(&mod_id.as_bytes()[0], &name[0], name.len() as u32, arg_len)
        };

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
        Ret::Archived: Deserialize<Ret, Infallible>,
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

        let name_slice = name.as_bytes();

        let ret_len = unsafe {
            ext::t(
                &mod_id.as_bytes()[0],
                &name_slice[0],
                name_slice.len() as u32,
                arg_len,
            )
        };

        with_arg_buf(|buf| {
            let slice = &buf[..ret_len as usize];
            let ret = unsafe { archived_root::<Ret>(slice) };
            ret.deserialize(&mut Infallible).expect("Infallible")
        })
    }
}
