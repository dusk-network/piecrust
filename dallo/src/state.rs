// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use core::cell::UnsafeCell;
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

extern "C" {
    fn q(
        mod_id: *const u8,
        name: *const u8,
        name_len: u32,
        arg_len: u32,
    ) -> u32;
    fn t(
        mod_id: *const u8,
        name: *const u8,
        name_len: u32,
        arg_len: u32,
    ) -> u32;

    fn height() -> i32;
    fn caller() -> u32;
    fn emit(arg_len: u32);
}

fn extern_query(module_id: ModuleId, name: &str, arg_len: u32) -> u32 {
    let mod_ptr = module_id.as_ptr();
    let name_ptr = name.as_ptr();
    let name_len = name.as_bytes().len() as u32;
    unsafe { q(mod_ptr, name_ptr, name_len, arg_len) }
}

fn extern_transaction(module_id: ModuleId, name: &str, arg_len: u32) -> u32 {
    let mod_ptr = module_id.as_ptr();
    let name_ptr = name.as_ptr();
    let name_len = name.as_bytes().len() as u32;
    unsafe { t(mod_ptr, name_ptr, name_len, arg_len) }
}

use crate::ModuleId;
use core::ops::{Deref, DerefMut};

pub struct State<S> {
    inner: S,
    buffer: UnsafeCell<&'static mut [u64]>,
}

impl<S> State<S> {
    pub const fn new(inner: S, buffer: &'static mut [u64]) -> Self {
        State {
            inner,
            buffer: UnsafeCell::new(buffer),
        }
    }

    /// # Safety
    /// TODO write a good comment for why this is safe
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn buffer(&self) -> &mut [u8] {
        let buf = &mut **self.buffer.get();
        let len_in_bytes = buf.len() * 8;
        let first = &mut buf[0];
        let first_byte: &mut u8 = core::mem::transmute(first);

        core::slice::from_raw_parts_mut(first_byte, len_in_bytes)
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

impl<S> State<S> {
    pub fn query<Arg, Ret>(&self, mod_id: ModuleId, name: &str, arg: Arg) -> Ret
    where
        Arg: for<'a> Serialize<StandardBufSerializer<'a>>,
        Ret: Archive,
        Ret::Archived: StandardDeserialize<Ret>,
    {
        let arg_len = self.with_arg_buf(|buf| {
            let mut sbuf = [0u8; SCRATCH_BUF_BYTES];
            let scratch = BufferScratch::new(&mut sbuf);
            let ser = BufferSerializer::new(buf);
            let mut composite =
                CompositeSerializer::new(ser, scratch, rkyv::Infallible);

            composite.serialize_value(&arg).expect("infallible");
            composite.pos() as u32
        });

        let ret_len = extern_query(mod_id, name, arg_len);

        self.with_arg_buf(|buf| {
            let slice = &buf[..ret_len as usize];
            let ret = check_archived_root::<Ret>(slice)
                .expect("invalid return value");
            ret.deserialize(&mut Infallible).expect("Infallible")
        })
    }

    pub fn query_raw(&self, mod_id: ModuleId, raw: RawQuery) -> RawResult {
        self.with_arg_buf(|buf| {
            let bytes = raw.arg_bytes();
            buf[..bytes.len()].copy_from_slice(bytes);
        });

        let name = raw.name();
        let arg_len = raw.arg_bytes().len() as u32;
        let ret_len = extern_query(mod_id, name, arg_len);

        self.with_arg_buf(|buf| RawResult::new(&buf[..ret_len as usize]))
    }

    pub fn transact_raw(
        &self,
        mod_id: ModuleId,
        raw: RawTransaction,
    ) -> RawResult {
        self.with_arg_buf(|buf| {
            let bytes = raw.arg_bytes();
            buf[..bytes.len()].copy_from_slice(bytes);
        });

        let name = raw.name();
        let arg_len = raw.arg_bytes().len() as u32;
        let ret_len = extern_query(mod_id, name, arg_len);

        self.with_arg_buf(|buf| RawResult::new(&buf[..ret_len as usize]))
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
        let arg_len = self.with_arg_buf(|buf| {
            let mut sbuf = [0u8; SCRATCH_BUF_BYTES];
            let scratch = BufferScratch::new(&mut sbuf);
            let ser = BufferSerializer::new(buf);
            let mut composite =
                CompositeSerializer::new(ser, scratch, rkyv::Infallible);

            composite.serialize_value(&arg).unwrap();
            composite.pos() as u32
        });

        let ret_len = extern_transaction(mod_id, name, arg_len);

        self.with_arg_buf(|buf| {
            let slice = &buf[..ret_len as usize];
            let ret = check_archived_root::<Ret>(slice)
                .expect("invalid return value");
            ret.deserialize(&mut Infallible).expect("Infallible")
        })
    }

    /// Return the current height.
    pub fn height(&self) -> u64 {
        self.with_arg_buf(|buf| {
            let ret_len = unsafe { height() };

            let ret = check_archived_root::<u64>(&buf[..ret_len as usize])
                .expect("invalid height");
            ret.deserialize(&mut Infallible).expect("Infallible")
        })
    }

    /// Return the ID of the calling module. The returned id will be
    /// uninitialized if there is no caller - meaning this is the first module
    /// to be called.
    pub fn caller(&self) -> ModuleId {
        self.with_arg_buf(|buf| {
            let ret_len = unsafe { caller() };
            let ret = check_archived_root::<ModuleId>(&buf[..ret_len as usize])
                .expect("invalid caller");
            ret.deserialize(&mut Infallible).expect("Infallible")
        })
    }

    /// Emits an event with the given data.
    pub fn emit<D>(&self, data: D)
    where
        for<'a> D: Serialize<StandardBufSerializer<'a>>,
    {
        self.with_arg_buf(|buf| {
            let mut sbuf = [0u8; SCRATCH_BUF_BYTES];
            let scratch = BufferScratch::new(&mut sbuf);
            let ser = BufferSerializer::new(buf);
            let mut composite =
                CompositeSerializer::new(ser, scratch, rkyv::Infallible);

            composite.serialize_value(&data).unwrap();
            let arg_len = composite.pos() as u32;

            unsafe { emit(arg_len) }
        });
    }

    pub fn with_arg_buf<F, R>(&self, f: F) -> R
    where
        F: Fn(&mut [u8]) -> R,
    {
        f(unsafe { self.buffer() })
    }
}
