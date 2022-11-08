// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use rkyv::{
    archived_root,
    ser::serializers::{BufferScratch, BufferSerializer, CompositeSerializer},
    ser::Serializer,
    Archive, Archived, Deserialize, Infallible, Serialize,
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
        pub(crate) fn hq(name: *const u8, name_len: u32, arg_len: u32) -> u32;
        pub(crate) fn hd(name: *const u8, name_len: u32) -> u32;
        pub(crate) fn t(
            mod_id_ofs: *const u8,
            name: *const u8,
            name_len: u32,
            arg_len: u32,
        ) -> u32;

        pub(crate) fn height();
        pub(crate) fn caller();
        pub(crate) fn emit(arg_len: u32);
        pub(crate) fn limit() -> u64;
        pub(crate) fn spent() -> u64;
    }
}

use crate::ModuleId;
use core::ops::{Deref, DerefMut};

#[derive(Debug, Archive, Serialize, Deserialize)]
pub enum ModuleError {
    Panic,
    OutOfGas,
}

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

pub fn host_query<Arg, Ret>(name: &str, arg: Arg) -> Ret
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

    let name_ptr = name.as_bytes().as_ptr() as *const u8;
    let name_len = name.as_bytes().len() as u32;

    let ret_len = unsafe { ext::hq(name_ptr, name_len, arg_len) };

    with_arg_buf(|buf| {
        let slice = &buf[..ret_len as usize];
        let ret = unsafe { archived_root::<Ret>(slice) };
        ret.deserialize(&mut Infallible).expect("Infallible")
    })
}

pub fn query<Arg, Ret>(
    mod_id: ModuleId,
    name: &str,
    arg: Arg,
) -> Result<Ret, ModuleError>
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
        Ok(ret.deserialize(&mut Infallible).expect("Infallible"))
    })
}

pub fn query_raw(
    mod_id: ModuleId,
    raw: RawQuery,
) -> Result<RawResult, ModuleError> {
    with_arg_buf(|buf| {
        let bytes = raw.arg_bytes();
        buf[..bytes.len()].copy_from_slice(bytes);
    });

    let name = raw.name_bytes();
    let arg_len = raw.arg_bytes().len() as u32;

    let ret_len = unsafe {
        ext::q(&mod_id.as_bytes()[0], &name[0], name.len() as u32, arg_len)
    };

    with_arg_buf(|buf| Ok(RawResult::new(&buf[..ret_len as usize])))
}

/// Returns data made available by the host under the given name. The type `D`
/// must be correctly specified, otherwise undefined behavior will occur.
pub fn host_data<D>(name: &str) -> D
where
    D: Archive,
    D::Archived: Deserialize<D, Infallible>,
{
    let name_slice = name.as_bytes();

    let name = name_slice.as_ptr();
    let name_len = name_slice.len() as u32;

    unsafe {
        let arg_pos = ext::hd(name, name_len) as usize;

        with_arg_buf(|buf| {
            let ret = archived_root::<D>(&buf[..arg_pos]);
            ret.deserialize(&mut Infallible).expect("Infallible")
        })
    }
}

/// Return the current height.
pub fn height() -> u64 {
    unsafe { ext::height() };
    with_arg_buf(|buf| {
        let ret = unsafe {
            archived_root::<u64>(&buf[..core::mem::size_of::<Archived<u64>>()])
        };
        ret.deserialize(&mut Infallible).expect("Infallible")
    })
}

/// Return the ID of the calling module. The returned id will be
/// uninitialized if there is no caller - meaning this is the first module
/// to be called.
pub fn caller() -> ModuleId {
    unsafe { ext::caller() };
    with_arg_buf(|buf| {
        let ret = unsafe {
            archived_root::<ModuleId>(
                &buf[..core::mem::size_of::<Archived<ModuleId>>()],
            )
        };
        ret.deserialize(&mut Infallible).expect("Infallible")
    })
}

pub fn limit() -> u64 {
    unsafe { ext::limit() }
}

pub fn spent() -> u64 {
    unsafe { ext::spent() }
}

impl<S> State<S> {
    /// Emits an event with the given data.
    pub fn emit<D>(&mut self, data: D)
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

    pub fn transact_raw(
        &mut self,
        mod_id: ModuleId,
        raw: RawTransaction,
    ) -> Result<RawResult, ModuleError> {
        // Necessary to avoid ruling out potential memory changes from recursive
        // calls
        core::hint::black_box(self);

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

        with_arg_buf(|buf| Ok(RawResult::new(&buf[..ret_len as usize])))
    }

    pub fn transact<Arg, Ret>(
        &mut self,
        mod_id: ModuleId,
        name: &str,
        arg: Arg,
    ) -> Result<Ret, ModuleError>
    where
        Arg: for<'a> Serialize<StandardBufSerializer<'a>>,
        Ret: Archive,
        Ret::Archived: Deserialize<Ret, Infallible>,
    {
        // Necessary to avoid ruling out potential memory changes from recursive
        // calls
        core::hint::black_box(self);

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
            Ok(ret.deserialize(&mut Infallible).expect("Infallible"))
        })
    }
}
