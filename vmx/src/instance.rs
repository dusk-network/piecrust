// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::ops::{Deref, DerefMut};

use bytecheck::CheckBytes;
use rkyv::{
    check_archived_root,
    ser::{
        serializers::{BufferScratch, BufferSerializer, CompositeSerializer},
        Serializer,
    },
    validation::validators::DefaultValidator,
    Archive, Deserialize, Infallible, Serialize,
};
use uplink::{ModuleId, SCRATCH_BUF_BYTES};
use wasmer::TypedFunction;

use crate::imports::DefaultImports;
use crate::module::WrappedModule;
use crate::session::Session;
use crate::types::{Error, StandardBufSerializer};

pub struct WrappedInstance {
    instance: wasmer::Instance,
    arg_buf_ofs: usize,
    store: wasmer::Store,
}

pub(crate) struct Env {
    self_id: ModuleId,
    session: Session,
}

impl Deref for Env {
    type Target = Session;

    fn deref(&self) -> &Self::Target {
        &self.session
    }
}

impl DerefMut for Env {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.session
    }
}

impl Env {
    pub fn self_instance(&self) -> WrappedInstance {
        self.session.instance(self.self_id)
    }
}

impl WrappedInstance {
    pub fn new(
        mut store: wasmer::Store,
        session: Session,
        id: ModuleId,
        wrap: &WrappedModule,
    ) -> Result<Self, Error> {
        println!("in wrapped instance new");

        let env = Env {
            self_id: id,
            session: session.clone(),
        };

        let imports = DefaultImports::default(&mut store, env);
        let module_bytes = wrap.as_bytes();

        let module =
            unsafe { wasmer::Module::deserialize(&store, module_bytes)? };

        println!("pre instance creation");

        let instance = wasmer::Instance::new(&mut store, &module, &imports)?;

        println!("post instance creation");

        match instance.exports.get_global("A")?.get(&mut store) {
            wasmer::Value::I32(ofs) => Ok(WrappedInstance {
                store,
                instance,
                arg_buf_ofs: ofs as usize,
            }),
            _ => todo!(),
        }
    }

    pub(crate) fn copy_argument(&mut self, arg: &[u8]) {
        self.with_arg_buffer(|buf| buf[..arg.len()].copy_from_slice(arg))
    }

    pub(crate) fn read_from_arg_buffer<T>(
        &self,
        arg_len: u32,
    ) -> Result<T, Error>
    where
        T: Archive,
        T::Archived: Deserialize<T, Infallible>
            + for<'b> CheckBytes<DefaultValidator<'b>>,
    {
        // TODO use bytecheck here
        self.with_arg_buffer(|abuf| {
            let slice = &abuf[..arg_len as usize];
            let ta: &T::Archived = check_archived_root::<T>(slice)?;
            let t = ta.deserialize(&mut Infallible).expect("Infallible");
            Ok(t)
        })
    }

    pub(crate) fn with_memory<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        let mem =
            self.instance.exports.get_memory("memory").expect(
                "memory export should be checked at module creation time",
            );
        let view = mem.view(&self.store);
        let memory_bytes = unsafe { view.data_unchecked() };
        f(memory_bytes)
    }

    fn with_memory_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mem =
            self.instance.exports.get_memory("memory").expect(
                "memory export should be checked at module creation time",
            );
        let view = mem.view(&self.store);
        let memory_bytes = unsafe { view.data_unchecked_mut() };
        f(memory_bytes)
    }

    pub(crate) fn with_arg_buffer<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        self.with_memory_mut(|memory_bytes| {
            let a = self.arg_buf_ofs;
            let b = uplink::ARGBUF_LEN;
            let begin = &mut memory_bytes[a..];
            let trimmed = &mut begin[..b];
            f(trimmed)
        })
    }

    pub(crate) fn write_to_arg_buffer<T>(&self, value: T) -> Result<u32, Error>
    where
        T: for<'b> Serialize<StandardBufSerializer<'b>>,
    {
        self.with_arg_buffer(|abuf| {
            let mut sbuf = [0u8; SCRATCH_BUF_BYTES];
            let scratch = BufferScratch::new(&mut sbuf);
            let ser = BufferSerializer::new(abuf);
            let mut ser =
                CompositeSerializer::new(ser, scratch, rkyv::Infallible);

            ser.serialize_value(&value)?;

            Ok(ser.pos() as u32)
        })
    }

    pub fn query(
        &mut self,
        method_name: &str,
        arg_len: u32,
    ) -> Result<u32, Error> {
        let fun: TypedFunction<u32, u32> = self
            .instance
            .exports
            .get_typed_function(&self.store, method_name)?;

        Ok(fun.call(&mut self.store, arg_len)?)
    }

    pub fn transact<Arg, Ret>(
        &mut self,
        method_name: &str,
        arg: Arg,
    ) -> Result<Ret, Error>
    where
        Arg: for<'b> Serialize<StandardBufSerializer<'b>>,
        Ret: Archive,
        Ret::Archived: Deserialize<Ret, Infallible>
            + for<'b> CheckBytes<DefaultValidator<'b>>,
    {
        let arg_len = self.write_to_arg_buffer(arg)?;

        let fun: TypedFunction<u32, u32> = self
            .instance
            .exports
            .get_typed_function(&self.store, method_name)?;
        let ret_len = fun.call(&mut self.store, arg_len)?;

        self.read_from_arg_buffer(ret_len)
    }
}
