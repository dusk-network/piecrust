// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

mod event;
mod native;
mod stack;
mod store;

pub use event::{Event, Receipt};
pub use native::NativeQuery;

use std::cell::UnsafeCell;
use std::collections::BTreeMap;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dallo::{
    ModuleId, StandardBufSerializer, StandardDeserialize, MODULE_ID_BYTES,
};
use native::NativeQueries;
use parking_lot::ReentrantMutex;
use rkyv::{Archive, Serialize};
use stack::CallStack;
use store::new_store;
use tempfile::tempdir;
use wasmer::{imports, Exports, Function, Val};

use crate::env::Env;
use crate::error::Error;
use crate::instance::Instance;
use crate::memory::MemHandler;
use crate::snapshot::{MemoryPath, Snapshot, SnapshotLike};
use crate::storage_helpers::module_id_to_name;
use crate::Error::PersistenceError;

const DEFAULT_POINT_LIMIT: u64 = 4096;
const POINT_PASS_PERCENTAGE: u64 = 93;

#[derive(Debug)]
pub struct WorldInner {
    environments: BTreeMap<ModuleId, Env>,
    native_queries: NativeQueries,
    storage_path: PathBuf,
    debug: Vec<String>,
    events: Vec<Event>,
    call_stack: CallStack,
    height: u64,
    limit: u64,
}

impl Deref for WorldInner {
    type Target = BTreeMap<ModuleId, Env>;

    fn deref(&self) -> &Self::Target {
        &self.environments
    }
}

impl DerefMut for WorldInner {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.environments
    }
}

#[derive(Debug, Clone)]
pub struct World(Arc<ReentrantMutex<UnsafeCell<WorldInner>>>);

impl World {
    pub fn new<P>(path: P) -> Self
    where
        P: Into<PathBuf>,
    {
        World(Arc::new(ReentrantMutex::new(UnsafeCell::new(WorldInner {
            environments: BTreeMap::new(),
            native_queries: NativeQueries::new(),
            storage_path: path.into(),
            events: vec![],
            debug: vec![],
            call_stack: CallStack::default(),
            height: 0,
            limit: DEFAULT_POINT_LIMIT,
        }))))
    }

    pub fn ephemeral() -> Result<Self, Error> {
        Ok(World(Arc::new(ReentrantMutex::new(UnsafeCell::new(
            WorldInner {
                environments: BTreeMap::new(),
                native_queries: NativeQueries::new(),
                storage_path: tempdir()
                    .map_err(PersistenceError)?
                    .path()
                    .into(),
                events: vec![],
                debug: vec![],
                call_stack: CallStack::default(),
                height: 0,
                limit: DEFAULT_POINT_LIMIT,
            },
        )))))
    }

    pub fn persist(&self) -> Result<(), Error> {
        let guard = self.0.lock();
        let w = unsafe { &mut *guard.get() };
        for (module_id, environment) in w.environments.iter() {
            let memory_path = MemoryPath::new(self.memory_path(module_id));
            let snapshot = Snapshot::new(&memory_path)?;
            environment.inner_mut().set_snapshot_id(snapshot.id());
            snapshot.save(&memory_path)?;
        }
        Ok(())
    }

    pub fn restore(&self) -> Result<(), Error> {
        let guard = self.0.lock();
        let w = unsafe { &mut *guard.get() };
        for (module_id, environment) in w.environments.iter() {
            let memory_path = MemoryPath::new(self.memory_path(module_id));
            if let Some(snapshot_id) = environment.inner().snapshot_id() {
                let snapshot = Snapshot::from_id(*snapshot_id, &memory_path)?;
                snapshot.load(&memory_path)?;
                println!(
                    "restored state of module: {:?} from file: {:?}",
                    module_id_to_name(*module_id),
                    snapshot.path()
                );
            }
        }
        Ok(())
    }

    pub fn memory_path(&self, module_id: &ModuleId) -> PathBuf {
        self.storage_path().join(module_id_to_name(*module_id))
    }

    pub fn deploy(&mut self, bytecode: &[u8]) -> Result<ModuleId, Error> {
        let id_bytes: [u8; MODULE_ID_BYTES] = blake3::hash(bytecode).into();
        let id = ModuleId::from(id_bytes);

        let store = new_store(
            self.storage_path().join(module_id_to_name(id)).as_path(),
        );
        let module = wasmer::Module::new(&store, bytecode)?;

        let mut env = Env::uninitialized();

        let imports = imports! {
            "env" => {
                "alloc" => Function::new_native_with_env(&store, env.clone(), host_alloc),
                "dealloc" => Function::new_native_with_env(&store, env.clone(), host_dealloc),

                "snap" => Function::new_native_with_env(&store, env.clone(), host_snapshot),

                "q" => Function::new_native_with_env(&store, env.clone(), host_query),
                "nq" => Function::new_native_with_env(&store, env.clone(), host_native_query),
                "t" => Function::new_native_with_env(&store, env.clone(), host_transact),

                "height" => Function::new_native_with_env(&store, env.clone(), host_height),
                "host_debug" => Function::new_native_with_env(&store, env.clone(), host_debug),
        "host_panic" => Function::new_native_with_env(&store, env.clone(), host_panic),
                "emit" => Function::new_native_with_env(&store, env.clone(), host_emit),
                "caller" => Function::new_native_with_env(&store, env.clone(), host_caller),
                "limit" => Function::new_native_with_env(&store, env.clone(), host_limit),
                "spent" => Function::new_native_with_env(&store, env.clone(), host_spent),
            }
        };

        let instance = wasmer::Instance::new(&module, &imports)?;

        let arg_buf_ofs = global_i32(&instance.exports, "A")?;

        // TODO: We should check these buffers have the correct length.
        let self_id_ofs = global_i32(&instance.exports, "SELF_ID")?;

        let heap_base = global_i32(&instance.exports, "__heap_base")?;

        // check buffer alignment
        // debug_assert_eq!(arg_buf_ofs % 8, 0);

        // We need to read the actual value of AL from the offset into memory

        let instance = Instance::new(
            id,
            instance,
            self.clone(),
            MemHandler::new(heap_base as usize),
            arg_buf_ofs,
            heap_base,
            self_id_ofs,
        );
        instance.write_self_id(id);

        env.initialize(instance);

        let guard = self.0.lock();
        let w = unsafe { &mut *guard.get() };
        w.insert(id, env);

        Ok(id)
    }

    /// Registers a [`NativeQuery`] with the given `name`.
    pub fn register_native_query<Q>(&mut self, name: &'static str, query: Q)
    where
        Q: 'static + NativeQuery,
    {
        let guard = self.0.lock();
        let w = unsafe { &mut *guard.get() };

        w.native_queries.insert(name, query);
    }

    pub fn query<Arg, Ret>(
        &self,
        m_id: ModuleId,
        name: &str,
        arg: Arg,
    ) -> Result<Receipt<Ret>, Error>
    where
        Arg: for<'a> Serialize<StandardBufSerializer<'a>>,
        Ret: Archive,
        Ret::Archived: StandardDeserialize<Ret>,
    {
        let guard = self.0.lock();
        let w = unsafe { &mut *guard.get() };

        w.call_stack = CallStack::new(m_id, w.limit);

        let instance = w.get(&m_id).expect("invalid module id").inner();
        instance.set_remaining_points(w.limit);

        let ret = instance.query(name, arg)?;
        let remaining = instance.remaining_points();

        let events = mem::take(&mut w.events);
        let debug = mem::take(&mut w.debug);

        Ok(Receipt::new(ret, events, debug, w.limit - remaining))
    }

    pub fn transact<Arg, Ret>(
        &mut self,
        m_id: ModuleId,
        name: &str,
        arg: Arg,
    ) -> Result<Receipt<Ret>, Error>
    where
        Arg: for<'a> Serialize<StandardBufSerializer<'a>> + core::fmt::Debug,
        Ret: Archive,
        Ret::Archived: StandardDeserialize<Ret>,
    {
        let w = self.0.lock();
        let w = unsafe { &mut *w.get() };

        w.call_stack = CallStack::new(m_id, w.limit);

        let instance = w.get(&m_id).expect("invalid module id").inner_mut();
        instance.set_remaining_points(w.limit);

        let ret = instance.transact(name, arg)?;
        let remaining = instance.remaining_points();

        let events = mem::take(&mut w.events);
        let debug = mem::take(&mut w.debug);

        Ok(Receipt::new(ret, events, debug, w.limit - remaining))
    }

    /// Set the height available to modules.
    pub fn set_height(&mut self, height: u64) {
        let w = self.0.lock();
        let w = unsafe { &mut *w.get() };

        w.height = height;
    }

    /// Set the point limit for the next call.
    pub fn set_point_limit(&mut self, limit: u64) {
        let w = self.0.lock();
        let w = unsafe { &mut *w.get() };

        w.limit = limit;
    }

    fn perform_query(
        &self,
        name: &str,
        caller_id: ModuleId,
        callee_id: ModuleId,
        arg_len: u32,
    ) -> Result<u32, Error> {
        let guard = self.0.lock();
        let w = unsafe { &mut *guard.get() };

        let caller = w.get(&caller_id).expect("oh no").inner();

        let remaining = caller.remaining_points();
        let limit = remaining * POINT_PASS_PERCENTAGE / 100;

        w.call_stack.push(callee_id, limit);

        let caller = w.get(&caller_id).expect("oh no").inner();
        let callee = w.get(&callee_id).expect("no oh").inner();

        callee.set_remaining_points(limit);

        let mut min_len = 0;

        caller.with_arg_buffer(|buf_caller| {
            callee.with_arg_buffer(|buf_callee| {
                min_len = std::cmp::min(buf_caller.len(), buf_callee.len());
                buf_callee[..min_len].copy_from_slice(&buf_caller[..min_len]);
            })
        });

        let ret_ofs = callee.perform_query(name, arg_len)?;

        callee.with_arg_buffer(|buf_callee| {
            caller.with_arg_buffer(|buf_caller| {
                buf_caller[..min_len].copy_from_slice(&buf_callee[..min_len]);
            })
        });

        let callee_used = limit - callee.remaining_points();
        caller.set_remaining_points(remaining - callee_used);

        w.call_stack.pop();

        Ok(ret_ofs)
    }

    fn native_query(
        &self,
        name: &str,
        buf: &mut [u8],
        len: u32,
    ) -> Option<u32> {
        let guard = self.0.lock();
        let w = unsafe { &*guard.get() };

        w.native_queries.call(name, buf, len)
    }

    fn perform_transaction(
        &self,
        name: &str,
        caller_id: ModuleId,
        callee_id: ModuleId,
        arg_len: u32,
    ) -> Result<u32, Error> {
        let guard = self.0.lock();
        let w = unsafe { &mut *guard.get() };

        let caller = w.get(&caller_id).expect("oh no").inner();

        let remaining = caller.remaining_points();
        let limit = remaining * POINT_PASS_PERCENTAGE / 100;

        w.call_stack.push(callee_id, limit);

        let caller = w.get(&caller_id).expect("oh no").inner();
        let callee = w.get(&callee_id).expect("no oh").inner();

        callee.set_remaining_points(limit);

        caller.with_arg_buffer(|buf_caller| {
            callee.with_arg_buffer(|buf_callee| {
                let min_len = std::cmp::min(buf_caller.len(), buf_callee.len());
                buf_callee[..min_len].copy_from_slice(&buf_caller[..min_len]);
            })
        });

        let ret_len = callee.perform_transaction(name, arg_len)?;

        callee.with_arg_buffer(|buf_callee| {
            caller.with_arg_buffer(|buf_caller| {
                let min_len = std::cmp::min(buf_caller.len(), buf_callee.len());
                buf_caller[..min_len].copy_from_slice(&buf_callee[..min_len]);
            })
        });

        let callee_used = limit - callee.remaining_points();
        caller.set_remaining_points(remaining - callee_used);

        w.call_stack.pop();

        Ok(ret_len)
    }

    fn height(&self, instance: &Instance) -> Result<u32, Error> {
        let guard = self.0.lock();
        let w = unsafe { &*guard.get() };

        instance.write_to_arg_buffer(w.height)
    }

    fn emit(&self, module_id: ModuleId, data: Vec<u8>) {
        let guard = self.0.lock();
        let w = unsafe { &mut *guard.get() };

        w.events.push(Event::new(module_id, data));
    }

    pub(crate) fn debug(&self, string: String) {
        let guard = self.0.lock();
        let w = unsafe { &mut *guard.get() };

        println!("pushing string");

        w.debug.push(string);
    }

    fn limit(&self, instance: &Instance) -> Result<u32, Error> {
        let guard = self.0.lock();
        let w = unsafe { &*guard.get() };

        let limit = w.call_stack.limit();
        instance.write_to_arg_buffer(limit)
    }

    fn spent(&self, instance: &Instance) -> Result<u32, Error> {
        let guard = self.0.lock();
        let w = unsafe { &*guard.get() };

        let limit = w.call_stack.limit();
        let remaining = instance.remaining_points();

        instance.write_to_arg_buffer(limit - remaining)
    }

    fn caller(&self, instance: &Instance) -> Result<u32, Error> {
        let guard = self.0.lock();
        let w = unsafe { &*guard.get() };
        let caller = w.call_stack.caller();

        instance.write_to_arg_buffer(caller)
    }

    pub fn storage_path(&self) -> &Path {
        let guard = self.0.lock();
        let world_inner = unsafe { &*guard.get() };
        world_inner.storage_path.as_path()
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
    env.inner_mut()
        .alloc(amount as usize, align as usize)
        .try_into()
        .expect("i32 overflow")
}

fn host_dealloc(env: &Env, addr: i32) {
    env.inner_mut().dealloc(addr as usize)
}

// Debug helper to take a snapshot of the memory of the running process.
fn host_snapshot(env: &Env) {
    env.inner().snap()
}

fn host_query(
    env: &Env,
    module_id_adr: i32,
    method_name_adr: i32,
    method_name_len: u32,
    arg_len: u32,
) -> u32 {
    let module_id_adr = module_id_adr as usize;
    let method_name_adr = method_name_adr as usize;
    let method_name_len = method_name_len as usize;

    let instance = env.inner();
    let mut mod_id = ModuleId::uninitialized();

    let name = instance.with_memory(|buf| {
        mod_id.as_bytes_mut()[..].copy_from_slice(
            &buf[module_id_adr..][..core::mem::size_of::<ModuleId>()],
        );
        // performance: use a dedicated buffer here?
        core::str::from_utf8(&buf[method_name_adr..][..method_name_len])
            .expect("TODO, error out cleaner")
            .to_owned()
    });

    instance
        .world()
        .perform_query(&name, instance.id(), mod_id, arg_len)
        .expect("TODO: error handling")
}

fn host_native_query(
    env: &Env,
    name_adr: i32,
    name_len: u32,
    arg_len: u32,
) -> u32 {
    let name_adr = name_adr as usize;
    let name_len = name_len as usize;

    let instance = env.inner();

    let name = instance.with_memory(|buf| {
        // performance: use a dedicated buffer here?
        core::str::from_utf8(&buf[name_adr..][..name_len])
            .expect("TODO, error out cleaner")
            .to_owned()
    });

    instance
        .with_arg_buffer(|buf| {
            instance.world().native_query(&name, buf, arg_len)
        })
        .expect("TODO: error handling")
}

fn host_transact(
    env: &Env,
    module_id_adr: i32,
    method_name_adr: i32,
    method_name_len: u32,
    arg_len: u32,
) -> u32 {
    let module_id_adr = module_id_adr as usize;
    let method_name_adr = method_name_adr as usize;
    let method_name_len = method_name_len as usize;

    let instance = env.inner();
    let mut mod_id = ModuleId::uninitialized();

    let name = instance.with_memory(|buf| {
        mod_id.as_bytes_mut()[..].copy_from_slice(
            &buf[module_id_adr..][..core::mem::size_of::<ModuleId>()],
        );
        // performance: use a dedicated buffer here?
        core::str::from_utf8(&buf[method_name_adr..][..method_name_len])
            .expect("TODO, error out cleaner")
            .to_owned()
    });

    instance
        .world()
        .perform_transaction(&name, instance.id(), mod_id, arg_len)
        .expect("TODO: error handling")
}

fn host_height(env: &Env) -> u32 {
    let instance = env.inner();
    instance
        .world()
        .height(instance)
        .expect("TODO: error handling")
}

fn host_emit(env: &Env, arg_len: u32) {
    let instance = env.inner();
    let module_id = instance.id();

    let arg_len = arg_len as usize;

    let data = instance.with_arg_buffer(|buf| buf[..arg_len].to_vec());

    instance.world().emit(module_id, data);
}

fn host_spent(env: &Env) -> u32 {
    let instance = env.inner();
    instance
        .world()
        .spent(instance)
        .expect("TODO: error handling")
}

fn host_limit(env: &Env) -> u32 {
    let instance = env.inner();
    instance
        .world()
        .limit(instance)
        .expect("TODO: error handling")
}

fn host_caller(env: &Env) -> u32 {
    let instance = env.inner();
    instance
        .world()
        .caller(instance)
        .expect("TODO: error handling")
}

fn host_debug(env: &Env, ofs: i32, len: u32) {
    let instance = env.inner();
    instance.debug(ofs, len)
}

fn host_panic(env: &Env, ofs: i32, len: u32) {
    let instance = env.inner();
    instance.debug(ofs, len)
}
