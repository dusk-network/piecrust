use colored::*;
use dallo::{ModuleId, Ser, SCRATCH_BUF_BYTES};
use rkyv::{
    archived_value,
    ser::serializers::{BufferScratch, BufferSerializer, CompositeSerializer},
    ser::Serializer,
    Archive, Deserialize, Infallible, Serialize,
};
use std::{cell::UnsafeCell, sync::Arc};
use wasmer::{NativeFunc, WasmerEnv};

mod error;
mod memory;
mod world;

pub use world::World;

pub use error::Error;

use crate::memory::MemHandler;

#[derive(Debug)]
pub struct Instance {
    id: ModuleId,
    instance: wasmer::Instance,
    world: World,
    mem_handler: MemHandler,
    arg_buf_ofs: i32,
    arg_buf_len: i32,
    heap_base: i32,
}

#[derive(Debug)]
enum EnvInner {
    Uninitialized,
    Initialized(Instance),
}

#[derive(Clone, WasmerEnv, Debug)]
pub struct Env(Arc<UnsafeCell<EnvInner>>);

unsafe impl Sync for Env {}
unsafe impl Send for Env {}

impl Env {
    fn initialize(&mut self, instance: Instance) {
        unsafe {
            *self.0.get() = EnvInner::Initialized(instance);
        }
    }

    fn uninitialized() -> Self {
        Env(Arc::new(UnsafeCell::new(EnvInner::Uninitialized)))
    }

    fn inner(&self) -> &Instance {
        if let EnvInner::Initialized(ei) = unsafe { &*self.0.get() } {
            &ei
        } else {
            unreachable!("uninitialized env")
        }
    }

    fn inner_mut(&self) -> &mut Instance {
        if let EnvInner::Initialized(ref mut ei) = unsafe { &mut *self.0.get() } {
            ei
        } else {
            unreachable!("uninitialized env")
        }
    }
}

impl Instance {
    pub(crate) fn query<Arg, Ret>(&self, name: &str, arg: Arg) -> Result<Ret, Error>
    where
        Arg: for<'a> Serialize<Ser<'a>>,
        Ret: Archive,
        Ret::Archived: Deserialize<Ret, Infallible>,
    {
        let fun: NativeFunc<i32, i32> = self.instance.exports.get_native_function(name)?;

        let ret_pos = {
	    let arg_ofs = self.write_to_arg_buffer(arg)?;
            fun.call(arg_ofs as i32)?
        };

	self.read_from_arg_buffer(ret_pos as usize)
    }

    pub(crate) fn transact<Arg, Ret>(&mut self, name: &str, arg: Arg) -> Result<Ret, Error>
    where
        Arg: for<'a> Serialize<Ser<'a>>,
        Ret: Archive,
        Ret::Archived: Deserialize<Ret, Infallible>,
    {
        let fun: NativeFunc<i32, i32> = self.instance.exports.get_native_function(name)?;

        let ret_pos = {
            let entry = self.with_arg_buffer(|buf| {
                let mut sbuf = [0u8; SCRATCH_BUF_BYTES];
                let scratch = BufferScratch::new(&mut sbuf);
                let ser = BufferSerializer::new(buf);
                let mut composite = CompositeSerializer::new(ser, scratch, rkyv::Infallible);

                composite.serialize_value(&arg)
            })? as i32;

            fun.call(entry)?
        };

        Ok(self.with_arg_buffer(|buf| {
            let val = unsafe { archived_value::<Ret>(buf, ret_pos as usize) };
            val.deserialize(&mut Infallible).unwrap()
        }))
    }

    fn with_memory<F, R>(&self, f: F) -> R
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

    fn with_memory_mut<F, R>(&self, f: F) -> R
    where
        F: Fn(&mut [u8]) -> R,
    {
        let mem = self
            .instance
            .exports
            .get_memory("memory")
            .expect("memory export is checked at module creation time");
        let memory_bytes = unsafe { mem.data_unchecked_mut() };
        f(memory_bytes)
    }

    fn write_to_arg_buffer<T>(&self, value: T) -> Result<usize, error::Compo>
    where
        T: for<'a> Serialize<Ser<'a>>,
    {
        self.with_arg_buffer(|abuf| {
            let mut sbuf = [0u8; SCRATCH_BUF_BYTES];
            let scratch = BufferScratch::new(&mut sbuf);
            let ser = BufferSerializer::new(abuf);
            let mut composite = CompositeSerializer::new(ser, scratch, rkyv::Infallible);

            composite.serialize_value(&value)
        })
    }

    fn read_from_arg_buffer<T>(&self, arg_ofs: usize) -> Result<T, Error>
    where
        T: Archive,
        T::Archived: Deserialize<T, Infallible>,
    {
	// TODO use bytecheck here
        Ok(self.with_arg_buffer(|abuf| {
            let ta: &T::Archived = unsafe { archived_value::<T>(abuf, arg_ofs as usize) };
            ta.deserialize(&mut rkyv::Infallible).unwrap()
        }))
    }

    fn with_arg_buffer<F, R>(&self, f: F) -> R
    where
        F: Fn(&mut [u8]) -> R,
    {
        self.with_memory_mut(|memory_bytes| {
            let a = self.arg_buf_ofs as usize;
            let b = self.arg_buf_len as usize;
            let begin = &mut memory_bytes[a..];
            let trimmed = &mut begin[..b];
            f(trimmed)
        })
    }

    pub(crate) fn alloc(&mut self, amount: usize, align: usize) -> usize {
        self.mem_handler.alloc(amount, align)
    }

    pub(crate) fn dealloc(&mut self, _addr: usize) {
        ()
    }

    pub fn id(&self) -> ModuleId {
        self.id
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
                        let buf_end = buf_start + self.arg_buf_len as usize;
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

#[macro_export]
macro_rules! module_bytecode {
    ($name:literal) => {
        include_bytes!(concat!(
            "../target/wasm32-unknown-unknown/release/",
            $name,
            ".wasm"
        ))
    };
}
