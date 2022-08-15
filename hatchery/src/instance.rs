// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use colored::*;

use dallo::{
    ModuleId, StandardBufSerializer, StandardDeserialize, MODULE_ID_BYTES,
    SCRATCH_BUF_BYTES,
};
use rkyv::{
    check_archived_root,
    ser::serializers::{BufferScratch, BufferSerializer, CompositeSerializer},
    ser::Serializer,
    Archive, Deserialize, Infallible, Serialize,
};
use wasmer::NativeFunc;
use wasmer_middlewares::metering::{
    get_remaining_points, set_remaining_points, MeteringPoints,
};

use crate::error::*;
use crate::memory::MemHandler;
use crate::snapshot::SnapshotId;
use crate::world::World;

#[derive(Debug)]
pub struct Instance {
    id: ModuleId,
    instance: wasmer::Instance,
    world: World,
    mem_handler: MemHandler,
    arg_buf_ofs: i32,
    heap_base: i32,
    self_id_ofs: i32,
    snapshot_id: Option<SnapshotId>,
}

impl Instance {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        id: ModuleId,
        instance: wasmer::Instance,
        world: World,
        mem_handler: MemHandler,
        arg_buf_ofs: i32,
        heap_base: i32,
        self_id_ofs: i32,
    ) -> Self {
        Instance {
            id,
            instance,
            world,
            mem_handler,
            arg_buf_ofs,
            heap_base,
            self_id_ofs,
            snapshot_id: None,
        }
    }

    pub(crate) fn query<Arg, Ret>(
        &self,
        name: &str,
        arg: Arg,
    ) -> Result<Ret, Error>
    where
        Arg: for<'a> Serialize<StandardBufSerializer<'a>>,
        Ret: Archive,
        Ret::Archived: StandardDeserialize<Ret>,
    {
        let ret_len = {
            let arg_len = self.write_to_arg_buffer(arg)?;
            self.perform_query(name, arg_len)
                .map_err(|e| map_call_err(self, e))?
        };

        self.read_from_arg_buffer(ret_len)
    }

    pub(crate) fn perform_query(
        &self,
        name: &str,
        arg_len: u32,
    ) -> Result<u32, Error> {
        let fun: NativeFunc<u32, u32> =
            self.instance.exports.get_native_function(name)?;
        Ok(fun.call(arg_len)?)
    }

    pub(crate) fn transact<Arg, Ret>(
        &mut self,
        name: &str,
        arg: Arg,
    ) -> Result<Ret, Error>
    where
        Arg: for<'a> Serialize<StandardBufSerializer<'a>> + core::fmt::Debug,
        Ret: Archive,
        Ret::Archived: StandardDeserialize<Ret>,
    {
        let ret_len = {
            let arg_len = self.write_to_arg_buffer(arg)?;
            self.perform_transaction(name, arg_len)
                .map_err(|e| map_call_err(self, e))?
        };

        self.read_from_arg_buffer(ret_len)
    }

    pub(crate) fn perform_transaction(
        &self,
        name: &str,
        arg_len: u32,
    ) -> Result<u32, Error> {
        let fun: NativeFunc<u32, u32> =
            self.instance.exports.get_native_function(name)?;
        Ok(fun.call(arg_len)?)
    }

    pub(crate) fn remaining_points(&self) -> u64 {
        match get_remaining_points(&self.instance) {
            MeteringPoints::Remaining(r) => r,
            MeteringPoints::Exhausted => 0,
        }
    }

    pub(crate) fn set_remaining_points(&self, points: u64) {
        set_remaining_points(&self.instance, points)
    }

    pub(crate) fn with_memory<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        let mem = self
            .instance
            .exports
            .get_memory("memory")
            .expect("memory export is checked at module creation time");
        let memory_bytes = unsafe { mem.data_unchecked() };

        f(memory_bytes)
    }

    pub(crate) fn with_memory_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mem =
            self.instance.exports.get_memory("memory").expect(
                "memory export should be checked at module creation time",
            );
        let memory_bytes = unsafe { mem.data_unchecked_mut() };
        f(memory_bytes)
    }

    pub(crate) fn write_self_id(&self, module_id: ModuleId) {
        let mem =
            self.instance.exports.get_memory("memory").expect(
                "memory export should be checked at module creation time",
            );

        let ofs = self.self_id_ofs as usize;

        let memory = unsafe { mem.data_unchecked_mut() };
        let self_id_buf = &mut memory[ofs..][..MODULE_ID_BYTES];

        self_id_buf.copy_from_slice(module_id.as_bytes());
    }

    pub(crate) fn write_to_arg_buffer<T>(&self, value: T) -> Result<u32, Error>
    where
        T: for<'a> Serialize<StandardBufSerializer<'a>>,
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

    fn read_from_arg_buffer<T>(&self, arg_len: u32) -> Result<T, Error>
    where
        T: Archive,
        T::Archived: StandardDeserialize<T>,
    {
        // TODO use bytecheck here
        self.with_arg_buffer(|abuf| {
            let slice = &abuf[..arg_len as usize];
            let ta: &T::Archived = check_archived_root::<T>(slice)?;
            let t = ta.deserialize(&mut Infallible).expect("Infallible");
            Ok(t)
        })
    }

    pub(crate) fn with_arg_buffer<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        self.with_memory_mut(|memory_bytes| {
            let a = self.arg_buf_ofs as usize;
            let b = dallo::ARGBUF_LEN;
            let begin = &mut memory_bytes[a..];
            let trimmed = &mut begin[..b];
            f(trimmed)
        })
    }

    pub(crate) fn alloc(&mut self, amount: usize, align: usize) -> usize {
        self.mem_handler.alloc(amount, align)
    }

    pub(crate) fn dealloc(&mut self, _addr: usize) {}

    pub fn id(&self) -> ModuleId {
        self.id
    }

    pub(crate) fn set_snapshot_id(&mut self, snapshot_id: SnapshotId) {
        self.snapshot_id = Some(snapshot_id);
    }
    pub fn snapshot_id(&self) -> Option<&SnapshotId> {
        self.snapshot_id.as_ref()
    }
    pub(crate) fn world(&self) -> &World {
        &self.world
    }

    pub fn snap(&self) {
        let mem = self
            .instance
            .exports
            .get_memory("memory")
            .expect("memory export is checked at module creation time");

        println!("memory snapshot");

        let maybe_interesting = unsafe { mem.data_unchecked_mut() };

        const CSZ: usize = 128;
        const RSZ: usize = 16;

        for (chunk_nr, chunk) in maybe_interesting.chunks(CSZ).enumerate() {
            if chunk[..] != [0; CSZ][..] {
                for (row_nr, row) in chunk.chunks(RSZ).enumerate() {
                    let ofs = chunk_nr * CSZ + row_nr * RSZ;

                    print!("{:08x}:", ofs);

                    for (i, byte) in row.iter().enumerate() {
                        if i % 4 == 0 {
                            print!(" ");
                        }

                        let buf_start = self.arg_buf_ofs as usize;
                        let buf_end = buf_start + dallo::ARGBUF_LEN as usize;
                        let heap_base = self.heap_base as usize;

                        if ofs + i >= buf_start && ofs + i < buf_end {
                            print!("{}", format!("{:02x}", byte).red());
                            print!(" ");
                        } else if ofs + i >= heap_base {
                            print!("{}", format!("{:02x} ", byte).green());
                        } else {
                            print!("{:02x} ", byte)
                        }
                    }

                    println!();
                }
            }
        }
    }
}

fn map_call_err(instance: &Instance, err: Error) -> Error {
    match err {
        e @ Error::RuntimeError(_) => {
            match get_remaining_points(&instance.instance) {
                MeteringPoints::Remaining(_) => e,
                MeteringPoints::Exhausted => Error::OutOfPoints(instance.id),
            }
        }
        e => e,
    }
}
