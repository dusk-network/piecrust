// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

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
use wasmer::{imports, TypedFunction};

use dallo::SCRATCH_BUF_BYTES;

use crate::module::WrappedModule;
use crate::types::{Error, StandardBufSerializer};

pub struct WrappedInstance {
    instance: wasmer::Instance,
    arg_buf_ofs: usize,
    store: wasmer::Store,
}

impl WrappedInstance {
    pub fn new(wrap: &WrappedModule) -> Result<Self, Error> {
        let imports = imports! {};
        let module_bytes = wrap.as_bytes();

        let mut store = wasmer::Store::default();
        let module =
            unsafe { wasmer::Module::deserialize(&store, module_bytes)? };

        let instance = wasmer::Instance::new(&mut store, &module, &imports)?;
        match instance.exports.get_global("A")?.get(&mut store) {
            wasmer::Value::I32(ofs) => Ok(WrappedInstance {
                store,
                instance,
                arg_buf_ofs: ofs as usize,
            }),
            _ => todo!(),
        }
    }

    fn read_from_arg_buffer<T>(&self, arg_len: u32) -> Result<T, Error>
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

    fn with_arg_buffer<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        self.with_memory_mut(|memory_bytes| {
            let a = self.arg_buf_ofs;
            let b = dallo::ARGBUF_LEN;
            let begin = &mut memory_bytes[a..];
            let trimmed = &mut begin[..b];
            f(trimmed)
        })
    }

    fn write_to_arg_buffer<T>(&self, value: T) -> Result<u32, Error>
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

    pub fn query<Arg, Ret>(
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
