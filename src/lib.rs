use std::{cell::UnsafeCell, sync::Arc};

use rkyv::{
    archived_value,
    ser::{serializers::BufferSerializer, Serializer},
    Archive, Deserialize, Infallible, Serialize,
};
use wasmer::{imports, Exports, Function, NativeFunc, Val, WasmerEnv};

mod error;
mod memory;

pub use error::Error;

use crate::memory::MemHandler;

#[derive(Debug)]
enum EnvInner {
    Uninitialized,
    Initialized {
        instance: wasmer::Instance,
        mem_handler: MemHandler,
        arg_buf_ofs: i32,
        arg_buf_len: i32,
    },
}

#[derive(Clone, WasmerEnv)]
pub struct Env(Arc<UnsafeCell<EnvInner>>);

unsafe impl Sync for Env {}
unsafe impl Send for Env {}

impl Env {
    fn initialize(
        &mut self,
        instance: wasmer::Instance,
        arg_buf_ofs: i32,
        arg_buf_len: i32,
        heap_base: i32,
    ) {
        unsafe {
            *self.0.get() = EnvInner::Initialized {
                instance,
                mem_handler: MemHandler::new(heap_base as usize),
                arg_buf_ofs,
                arg_buf_len,
            };
        }
    }

    fn uninitialized() -> Self {
        Env(Arc::new(UnsafeCell::new(EnvInner::Uninitialized)))
    }

    pub fn new(bytecode: &[u8]) -> Result<Self, Error> {
        let store = wasmer::Store::default();
        let module = wasmer::Module::new(&store, bytecode)?;

        let mut env = Env::uninitialized();

        let real_imports = imports! {
            "env" => {
                "alloc" => Function::new_native_with_env(&store, env.clone(), host_alloc),
                "snap" => Function::new_native_with_env(&store, env.clone(), host_snapshot),
            }
        };

        let instance = wasmer::Instance::new(&module, &real_imports)?;

        let arg_buf_ofs = global_i32(&instance.exports, "A")?;
        let arg_buf_len_pos = global_i32(&instance.exports, "AL")?;
        let heap_base = global_i32(&instance.exports, "__heap_base")?;

        // We need to read the actual value of AL from the offset into memory

        let mem = instance.exports.get_memory("memory")?;
        let data = &unsafe { mem.data_unchecked() }[arg_buf_len_pos as usize..][..4];

        let arg_buf_len: i32 = unsafe { archived_value::<i32>(data, 0) }
            .deserialize(&mut Infallible)
            .expect("infallible");

        println!("arg_buf_len {:?}", arg_buf_len);

        env.initialize(instance, arg_buf_ofs, arg_buf_len, heap_base);

        Ok(env)
    }

    pub fn query<Arg, Ret>(&self, name: &str, arg: Arg) -> Result<Ret, Error>
    where
        Arg: for<'a> Serialize<BufferSerializer<&'a mut [u8]>>,
        Ret: Archive + core::fmt::Debug,
        Ret::Archived: Deserialize<Ret, Infallible> + core::fmt::Debug,
    {
        if let EnvInner::Initialized { instance, .. } = unsafe { &*self.0.get() } {
            let fun: NativeFunc<i32, i32> = instance.exports.get_native_function(name)?;

            let ret_pos = {
                let entry = self.with_arg_buffer(|buf| {
                    let mut serializer = BufferSerializer::new(buf);
                    serializer.serialize_value(&arg)
                })? as i32;

                fun.call(entry)?
            };

            println!("mid query snap!");
            self.snap();

            Ok(self.with_arg_buffer(|buf| {
                let val = unsafe { archived_value::<Ret>(buf, ret_pos as usize) };
                val.deserialize(&mut Infallible).unwrap()
            }))
        } else {
            unreachable!("Call on uninitialized environment")
        }
    }

    pub fn transact<Arg, Ret>(&mut self, name: &str, arg: Arg) -> Result<Ret, Error>
    where
        Arg: for<'a> Serialize<BufferSerializer<&'a mut [u8]>>,
        Ret: Archive + core::fmt::Debug,
        Ret::Archived: Deserialize<Ret, Infallible> + core::fmt::Debug,
    {
        if let EnvInner::Initialized { instance, .. } = unsafe { &*self.0.get() } {
            let fun: NativeFunc<i32, i32> = instance.exports.get_native_function(name)?;

            let ret_pos = {
                let entry = self.with_arg_buffer(|buf| {
                    let mut serializer = BufferSerializer::new(buf);
                    serializer.serialize_value(&arg)
                })? as i32;

                fun.call(entry)?
            };

            Ok(self.with_arg_buffer(|buf| {
                let val = unsafe { archived_value::<Ret>(buf, ret_pos as usize) };
                val.deserialize(&mut Infallible).unwrap()
            }))
        } else {
            unreachable!("Call on uninitialized environment")
        }
    }

    fn with_arg_buffer<F, R>(&self, f: F) -> R
    where
        F: Fn(&mut [u8]) -> R,
    {
        if let EnvInner::Initialized {
            instance,
            arg_buf_ofs,
            arg_buf_len,
            ..
        } = unsafe { &*self.0.get() }
        {
            // copy the argument bytes to the arg/ret buffer of the module.
            let mem = instance
                .exports
                .get_memory("memory")
                .expect("memory export is checked at module creation time");
            let memory_bytes = unsafe { mem.data_unchecked_mut() };

            let a = *arg_buf_ofs as usize;
            let b = *arg_buf_len as usize;

            let begin = &mut memory_bytes[a..];
            let trimmed = &mut begin[..b];
            f(trimmed)
        } else {
            unreachable!("Call on uninitialized environment")
        }
    }

    pub fn alloc(&self, amount: usize, align: usize) -> usize {
        if let EnvInner::Initialized { mem_handler, .. } = unsafe { &mut *self.0.get() } {
            mem_handler.alloc(amount, align)
        } else {
            unreachable!("Call on uninitialized environment")
        }
    }

    pub fn snap(&self) {
        if let EnvInner::Initialized { instance, .. } = unsafe { &*self.0.get() } {
            let mem = instance
                .exports
                .get_memory("memory")
                .expect("memory export is checked at module creation time");

            let maybe_interesting = unsafe { mem.data_unchecked_mut() };

            const CSZ: usize = 128;

            let mut zeroes = 0;
            for chunk in maybe_interesting.chunks(CSZ) {
                if chunk[..] != [0; CSZ][..] {
                    if zeroes > 1024 {
                        println!("- {} zeroes ({:?} kibi) -", zeroes, zeroes as f64 / 1024.0);
                    } else if zeroes > 0 {
                        println!("- {} zeroes -", zeroes);
                    }
                    zeroes = 0;
                    println!("{}", pretty_hex::pretty_hex(&chunk));
                } else {
                    zeroes += CSZ
                }
            }
        } else {
            unreachable!("Call on uninitialized environment")
        }
    }
}

fn global_i32(exports: &Exports, name: &str) -> Result<i32, Error> {
    if let Val::I32(i) = exports.get_global(name)?.get() {
        Ok(i)
    } else {
        Err(Error::MissingModuleExport)
    }
}

fn host_alloc(env: &Env, amount: i32, align: i32) -> i32 {
    env.alloc(amount as usize, align as usize)
        .try_into()
        .expect("i32 overflow")
}

// Debug helper to take a snapshot of the memory of the running process.
fn host_snapshot(env: &Env, amount: i32, align: i32) -> i32 {
    env.alloc(amount as usize, align as usize)
        .try_into()
        .expect("i32 overflow")
}

#[macro_export]
macro_rules! module {
    ($name:literal) => {
        hatchery::Env::new(include_bytes!(concat!(
            "../target/wasm32-unknown-unknown/release/",
            $name,
            ".wasm"
        )))
    };
}
