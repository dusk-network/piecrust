use rkyv::{
    archived_value,
    ser::serializers::{BufferScratch, BufferSerializer, CompositeSerializer},
    ser::Serializer,
    Archive, Deserialize, Infallible, Serialize,
};

use crate::Ser;

extern "C" {
    fn q(mod_id: *const u8, name: *const u8, len: i32, arg_ofs: i32) -> i32;
    fn t(mod_id: *const u8, name: *const u8, len: i32, arg_ofs: i32) -> i32;
}

fn extern_query(module_id: ModuleId, name: &str, arg_ofs: i32) -> i32 {
    let mod_ptr = module_id.as_ptr();
    let nme_ptr = name.as_ptr();
    let nme_len = name.as_bytes().len() as i32;
    unsafe { q(mod_ptr, nme_ptr, nme_len, arg_ofs) }
}

fn extern_transaction(module_id: ModuleId, name: &str, arg_ofs: i32) -> i32 {
    let mod_ptr = module_id.as_ptr();
    let nme_ptr = name.as_ptr();
    let nme_len = name.as_bytes().len() as i32;
    unsafe { t(mod_ptr, nme_ptr, nme_len, arg_ofs) }
}

use crate::ModuleId;
use core::ops::{Deref, DerefMut};

pub struct State<S> {
    inner: S,
    buffer: *mut [u8],
}

impl<S> State<S> {
    pub const fn new(inner: S, buffer: &'static mut [u8]) -> Self {
        State { inner, buffer }
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
        Arg: for<'a> Serialize<Ser<'a>>,
        Ret: Archive,
        Ret::Archived: Deserialize<Ret, Infallible>,
    {
        let arg_ofs = self.with_arg_buf(|buf| {
            let mut sbuf = [0u8; 16];
            let scratch = BufferScratch::new(&mut sbuf);
            let ser = BufferSerializer::new(buf);
            let mut composite =
                CompositeSerializer::new(ser, scratch, rkyv::Infallible);

            composite.serialize_value(&arg).unwrap() as i32
        });

        let ret_ofs = extern_query(mod_id, name, arg_ofs);

        self.with_arg_buf(|buf| {
            let ret = unsafe { archived_value::<Ret>(buf, ret_ofs as usize) };

            let de = ret.deserialize(&mut Infallible).expect("Infallible");

            de
        })
    }

    pub fn transact<Arg, Ret>(
        &mut self,
        mod_id: ModuleId,
        name: &str,
        arg: Arg,
    ) -> Ret
    where
        Arg: for<'a> Serialize<Ser<'a>>,
        Ret: Archive,
        Ret::Archived: Deserialize<Ret, Infallible>,
    {
        let arg_ofs = self.with_arg_buf(|buf| {
            let mut sbuf = [0u8; 16];
            let scratch = BufferScratch::new(&mut sbuf);
            let ser = BufferSerializer::new(buf);
            let mut composite =
                CompositeSerializer::new(ser, scratch, rkyv::Infallible);

            composite.serialize_value(&arg).unwrap() as i32
        });

        let ret_ofs = extern_transaction(mod_id, name, arg_ofs);

        self.with_arg_buf(|buf| {
            let ret = unsafe { archived_value::<Ret>(buf, ret_ofs as usize) };
            let de = ret.deserialize(&mut Infallible).expect("Infallible");
            de
        })
    }

    pub fn with_arg_buf<F, R>(&self, f: F) -> R
    where
        F: Fn(&mut [u8]) -> R,
    {
        f(unsafe { &mut *self.buffer })
    }
}
