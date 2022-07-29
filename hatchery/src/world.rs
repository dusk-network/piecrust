// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::cell::UnsafeCell;
use std::collections::BTreeMap;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dallo::{ModuleId, Ser, SnapshotId};
use parking_lot::ReentrantMutex;
use rkyv::{archived_value, Archive, Deserialize, Infallible, Serialize};
use snapshot::{MemoryEdge, Snapshot};
use tempfile::tempdir;
use wasmer::{imports, Exports, Function, Val};

use crate::env::Env;
use crate::error::Error;
use crate::instance::Instance;
use crate::memory::MemHandler;
use crate::snapshot;
use crate::storage_helpers::{
    combine_module_snapshot_names, module_id_to_name, snapshot_id_to_name,
};
use crate::Error::{MemoryError, PersistenceError};

#[derive(Debug)]
pub struct WorldInner {
    environments: BTreeMap<ModuleId, Env>,
    storage_path: PathBuf,
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
            storage_path: path.into(),
        }))))
    }

    pub fn ephemeral() -> Result<Self, Error> {
        Ok(World(Arc::new(ReentrantMutex::new(UnsafeCell::new(
            WorldInner {
                environments: BTreeMap::new(),
                storage_path: tempdir()
                    .map_err(PersistenceError)?
                    .path()
                    .into(),
            },
        )))))
    }

    /// Writes memory edge as a non-compressed snapshot
    pub fn create_snapshot(
        &self,
        module_id: ModuleId,
        out_snapshot_id: SnapshotId,
    ) -> Result<(), Error> {
        let memory_edge = MemoryEdge::new(
            self.storage_path()
                .join(module_id_to_name(module_id))
                .as_path(),
        );
        let out_snapshot = Snapshot::new(out_snapshot_id, &memory_edge);
        out_snapshot.write(&memory_edge)?;
        Ok(())
    }

    /// Writes compressed snapshot of a diff between memory edge and a given
    /// base (non-compressed) snapshot
    pub fn create_compressed_snapshot(
        &self,
        module_id: ModuleId,
        base_snapshot_id: SnapshotId,
        out_snapshot_id: SnapshotId,
    ) -> Result<(), Error> {
        let memory_edge = MemoryEdge::new(
            self.storage_path()
                .join(module_id_to_name(module_id))
                .as_path(),
        );
        let base_snapshot = Snapshot::new(base_snapshot_id, &memory_edge);
        let out_snapshot = Snapshot::new(out_snapshot_id, &memory_edge);
        out_snapshot.write_compressed(&memory_edge, &base_snapshot)?;
        Ok(())
    }

    /// Deploys module with a given non-compressed snapshot
    pub fn restore_from_snapshot(
        &mut self,
        bytecode: &[u8],
        mem_grow_by: u32,
        snapshot_id: SnapshotId,
    ) -> Result<ModuleId, Error> {
        fn build_filename(
            module_id: ModuleId,
            snapshot_id: SnapshotId,
        ) -> String {
            combine_module_snapshot_names(
                module_id_to_name(module_id),
                snapshot_id_to_name(snapshot_id),
            )
        }
        self.deploy_with_snapshot(bytecode, mem_grow_by, snapshot_id, build_filename)
    }

    /// Deploys module with a given base (non-compressed) snapshot and a
    /// compressed snapshot
    pub fn restore_from_compressed_snapshot(
        &mut self,
        bytecode: &[u8],
        mem_grow_by: u32,
        base_snapshot_id: SnapshotId,
        compressed_snapshot_id: SnapshotId,
    ) -> Result<ModuleId, Error> {
        self.deploy_with_compressed_snapshot(
            bytecode,
            mem_grow_by,
            base_snapshot_id,
            compressed_snapshot_id,
        )
    }

    /// Deploys module off the edge
    pub fn deploy(
        &mut self,
        bytecode: &[u8],
        mem_grow_by: u32,
    ) -> Result<ModuleId, Error> {
        fn build_filename(
            module_id: ModuleId,
            _snapshot_id: SnapshotId,
        ) -> String {
            module_id_to_name(module_id)
        }
        const EMPTY_SNAPSHOT_ID: SnapshotId = [0u8; 32];
        self.deploy_with_snapshot(
            bytecode,
            mem_grow_by,
            EMPTY_SNAPSHOT_ID,
            build_filename,
        )
    }

    /// Deploys module but first it decompresses snapshot into edge
    fn deploy_with_compressed_snapshot(
        &mut self,
        bytecode: &[u8],
        mem_grow_by: u32,
        base_snapshot_id: SnapshotId,
        compressed_snapshot_id: SnapshotId,
    ) -> Result<ModuleId, Error> {
        let module_id: ModuleId = blake3::hash(bytecode).into(); // todo - suboptimal that this has to be done here as well
        let full_path = self
            .storage_path()
            .join(module_id_to_name(module_id));
        let memory_edge_path = full_path.as_path();
        let compressed_snapshot = Snapshot::new(
            compressed_snapshot_id,
            &MemoryEdge::new(memory_edge_path),
        );
        let base_snapshot =
            Snapshot::new(base_snapshot_id, &MemoryEdge::new(memory_edge_path));
        let edge = Snapshot::from_edge(&MemoryEdge::new(memory_edge_path));
        compressed_snapshot.decompress(&base_snapshot, &edge)?;
        self.deploy(bytecode, mem_grow_by)
    }

    /// Deploys module with or without snapshot depending on the build_filename function
    fn deploy_with_snapshot(
        &mut self,
        bytecode: &[u8],
        mem_grow_by: u32,
        snapshot_id: SnapshotId,
        build_filename: fn(ModuleId, SnapshotId) -> String,
    ) -> Result<ModuleId, Error> {
        let id: ModuleId = blake3::hash(bytecode).into();
        println!(
            "deploy this path={:?}",
            self.storage_path().join(build_filename(id, snapshot_id))
        );
        let store = wasmer::Store::new_with_path(
            self.storage_path()
                .join(build_filename(id, snapshot_id))
                .as_path(),
        );
        let module = wasmer::Module::new(&store, bytecode)?;

        let mut env = Env::uninitialized();

        #[rustfmt::skip]
        let imports = imports! {
            "env" => {
                "alloc" => Function::new_native_with_env(&store, env.clone(), host_alloc),
		        "dealloc" => Function::new_native_with_env(&store, env.clone(), host_dealloc),

                "snap" => Function::new_native_with_env(&store, env.clone(), host_snapshot),

                "q" => Function::new_native_with_env(&store, env.clone(), host_query),
		        "t" => Function::new_native_with_env(&store, env.clone(), host_transact),
            }
        };

        let instance = wasmer::Instance::new(&module, &imports)?;

        let mem = instance.exports.get_memory("memory")?;
        if mem_grow_by != 0 {
            let _ = mem.grow(mem_grow_by).map_err(MemoryError)?;
        }

        let arg_buf_ofs = global_i32(&instance.exports, "A")?;
        let arg_buf_len_pos = global_i32(&instance.exports, "AL")?;
        let heap_base = global_i32(&instance.exports, "__heap_base")?;

        // We need to read the actual value of AL from the offset into memory

        let data =
            &unsafe { mem.data_unchecked() }[arg_buf_len_pos as usize..][..4];

        let arg_buf_len: i32 = unsafe { archived_value::<i32>(data, 0) }
            .deserialize(&mut Infallible)
            .expect("infallible");

        let instance = Instance::new(
            id,
            instance,
            self.clone(),
            MemHandler::new(heap_base as usize),
            arg_buf_ofs,
            arg_buf_len,
            heap_base,
        );

        env.initialize(instance);

        let guard = self.0.lock();
        let w = unsafe { &mut *guard.get() };
        w.insert(id, env);

        Ok(id)
    }

    pub fn query<Arg, Ret>(
        &self,
        m_id: ModuleId,
        name: &str,
        arg: Arg,
    ) -> Result<Ret, Error>
    where
        Arg: for<'a> Serialize<Ser<'a>>,
        Ret: Archive,
        Ret::Archived: Deserialize<Ret, Infallible>,
    {
        let guard = self.0.lock();
        let w = unsafe { &*guard.get() };

        w.get(&m_id)
            .expect("invalid module id")
            .inner()
            .query(name, arg)
    }

    pub fn transact<Arg, Ret>(
        &mut self,
        m_id: ModuleId,
        name: &str,
        arg: Arg,
    ) -> Result<Ret, Error>
    where
        Arg: for<'a> Serialize<Ser<'a>>,
        Ret: Archive,
        Ret::Archived: Deserialize<Ret, Infallible>,
    {
        let w = self.0.lock();
        let w = unsafe { &mut *w.get() };

        w.get_mut(&m_id)
            .expect("invalid module id")
            .inner_mut()
            .transact(name, arg)
    }

    fn perform_query(
        &self,
        name: &str,
        caller: ModuleId,
        callee: ModuleId,
        arg_ofs: i32,
    ) -> Result<i32, Error> {
        let guard = self.0.lock();
        let w = unsafe { &*guard.get() };

        let caller = w.get(&caller).expect("oh no").inner();
        let callee = w.get(&callee).expect("no oh").inner();

        let mut min_len = 0;

        caller.with_arg_buffer(|buf_caller| {
            callee.with_arg_buffer(|buf_callee| {
                min_len = std::cmp::min(buf_caller.len(), buf_callee.len());
                buf_callee[..min_len].copy_from_slice(&buf_caller[..min_len]);
            })
        });

        let ret_ofs = callee.perform_query(name, arg_ofs)?;

        callee.with_arg_buffer(|buf_callee| {
            caller.with_arg_buffer(|buf_caller| {
                buf_caller[..min_len].copy_from_slice(&buf_callee[..min_len]);
            })
        });

        Ok(ret_ofs)
    }

    fn perform_transaction(
        &self,
        name: &str,
        caller: ModuleId,
        callee: ModuleId,
        arg_ofs: i32,
    ) -> Result<i32, Error> {
        let guard = self.0.lock();
        let w = unsafe { &mut *guard.get() };

        let caller = w.get(&caller).expect("oh no").inner();
        let callee = w.get(&callee).expect("no oh").inner();

        caller.with_arg_buffer(|buf_caller| {
            callee.with_arg_buffer(|buf_callee| {
                let min_len = std::cmp::min(buf_caller.len(), buf_callee.len());
                buf_callee[..min_len].copy_from_slice(&buf_caller[..min_len]);
            })
        });

        let ret_ofs = callee.perform_transaction(name, arg_ofs)?;

        callee.with_arg_buffer(|buf_callee| {
            caller.with_arg_buffer(|buf_caller| {
                let min_len = std::cmp::min(buf_caller.len(), buf_callee.len());
                buf_caller[..min_len].copy_from_slice(&buf_callee[..min_len]);
            })
        });

        Ok(ret_ofs)
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
    method_name_len: i32,
    arg_ofs: i32,
) -> i32 {
    let module_id_adr = module_id_adr as usize;
    let method_name_adr = method_name_adr as usize;
    let method_name_len = method_name_len as usize;

    let instance = env.inner();
    let mut mod_id = ModuleId::default();
    // performance: use a dedicated buffer here?
    let mut name = String::new();

    instance.with_memory(|buf| {
        mod_id[..].copy_from_slice(
            &buf[module_id_adr..][..core::mem::size_of::<ModuleId>()],
        );
        let utf =
            core::str::from_utf8(&buf[method_name_adr..][..method_name_len])
                .expect("TODO, error out cleaner");
        name.push_str(utf)
    });

    instance
        .world()
        .perform_query(&name, instance.id(), mod_id, arg_ofs)
        .expect("TODO: error handling")
}

fn host_transact(
    env: &Env,
    module_id_adr: i32,
    method_name_adr: i32,
    method_name_len: i32,
    arg_ofs: i32,
) -> i32 {
    let module_id_adr = module_id_adr as usize;
    let method_name_adr = method_name_adr as usize;
    let method_name_len = method_name_len as usize;

    let instance = env.inner();
    let mut mod_id = ModuleId::default();
    // performance: use a dedicated buffer here?
    let mut name = String::new();

    instance.with_memory(|buf| {
        mod_id[..].copy_from_slice(
            &buf[module_id_adr..][..core::mem::size_of::<ModuleId>()],
        );
        let utf =
            core::str::from_utf8(&buf[method_name_adr..][..method_name_len])
                .expect("TODO, error out cleaner");
        name.push_str(utf)
    });

    instance
        .world()
        .perform_transaction(&name, instance.id(), mod_id, arg_ofs)
        .expect("TODO: error handling")
}
