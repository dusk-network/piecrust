// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::{self, Debug, Formatter};
use std::path::Path;
use std::sync::Arc;
use std::thread;

use dusk_wasmtime::{
    Config, Engine, ModuleVersionStrategy, OptLevel, Strategy,
    WasmBacktraceDetails,
};
use tempfile::tempdir;

use crate::session::{Session, SessionData};
use crate::store::ContractStore;
use crate::Error::{self, PersistenceError};

fn config() -> Config {
    let mut config = Config::new();

    // Neither WASM backtrace, nor native unwind info.
    config.wasm_backtrace(false);
    config.wasm_backtrace_details(WasmBacktraceDetails::Disable);

    config.native_unwind_info(false);

    // 512KiB of max stack is the default, but we want to be explicit about it.
    config.max_wasm_stack(0x80000);
    config.consume_fuel(true);

    config.strategy(Strategy::Cranelift);
    config.cranelift_opt_level(OptLevel::SpeedAndSize);
    // We need entirely deterministic computation
    config.cranelift_nan_canonicalization(true);

    // Host memory creator is set in the session.
    // config.with_host_memory()

    config.static_memory_forced(true);
    config.static_memory_guard_size(0);
    config.dynamic_memory_guard_size(0);
    config.guard_before_linear_memory(false);
    config.memory_init_cow(false);

    config
        .module_version(ModuleVersionStrategy::Custom(String::from("piecrust")))
        .expect("Module version should be valid");
    config.generate_address_map(false);
    config.macos_use_mach_ports(false);

    // Support 64-bit memories
    config.wasm_memory64(true);

    config
}

/// A handle to the piecrust virtual machine.
///
/// It is instantiated using [`new`] or [`ephemeral`], and can be used to spawn
/// multiple [`Session`]s using [`session`].
///
/// These sessions are synchronized with the help of a sync loop. [`Deletions`]
/// are assured to not delete any commits used as a base for sessions until
/// these are dropped. A handle to this loop is available at [`sync_thread`].
///
/// Users are encouraged to instantiate a `VM` once during the lifetime of their
/// program and spawn sessions as needed.
///
/// [`new`]: VM::new
/// [`ephemeral`]: VM::ephemeral
/// [`Session`]: Session
/// [`session`]: VM::session
/// [`Deletions`]: VM::delete_commit
/// [`sync_thread`]: VM::sync_thread
pub struct VM {
    engine: Engine,
    host_queries: HostQueries,
    store: ContractStore,
}

impl Debug for VM {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("VM")
            .field("config", self.engine.config())
            .field("host_queries", &self.host_queries)
            .field("store", &self.store)
            .finish()
    }
}

impl VM {
    /// Creates a new `VM`, reading the given `dir`ectory for existing commits
    /// and bytecode.
    ///
    /// The directory will be used to save any future session commits made by
    /// this `VM` instance.
    ///
    /// # Errors
    /// If the directory contains unparseable or inconsistent data.
    pub fn new<P: AsRef<Path>>(root_dir: P) -> Result<Self, Error> {
        let config = config();

        let engine = Engine::new(&config).expect(
            "Configuration should be valid since its set at compile time",
        );

        let store = ContractStore::new(engine.clone(), root_dir)
            .map_err(|err| PersistenceError(Arc::new(err)))?;

        Ok(Self {
            engine,
            host_queries: HostQueries::default(),
            store,
        })
    }

    /// Creates a new `VM` using a new temporary directory.
    ///
    /// Any session commits made by this machine should be considered discarded
    /// once this `VM` instance drops.
    ///
    /// # Errors
    /// If creating a temporary directory fails.
    pub fn ephemeral() -> Result<Self, Error> {
        let tmp = tempdir().map_err(|err| PersistenceError(Arc::new(err)))?;
        let tmp = tmp.path().to_path_buf();

        let config = config();

        let engine = Engine::new(&config).expect(
            "Configuration should be valid since its set at compile time",
        );

        let store = ContractStore::new(engine.clone(), tmp)
            .map_err(|err| PersistenceError(Arc::new(err)))?;

        Ok(Self {
            engine,
            host_queries: HostQueries::default(),
            store,
        })
    }

    /// Registers a [host `query`] with the given `name`.
    ///
    /// The query will be available to any session spawned *after* this was
    /// called.
    ///
    /// [host `query`]: HostQuery
    pub fn register_host_query<Q, S>(&mut self, name: S, query: Q)
    where
        Q: 'static + HostQuery,
        S: Into<Cow<'static, str>>,
    {
        self.host_queries.insert(name, query);
    }

    /// Spawn a [`Session`].
    ///
    /// # Errors
    /// If base commit is provided but does not exist.
    ///
    /// [`Session`]: Session
    pub fn session(
        &self,
        data: impl Into<SessionData>,
    ) -> Result<Session, Error> {
        let data = data.into();
        let contract_session = match data.base {
            Some(base) => self
                .store
                .session(base.into())
                .map_err(|err| PersistenceError(Arc::new(err)))?,
            _ => self.store.genesis_session(),
        };
        Ok(Session::new(
            self.engine.clone(),
            contract_session,
            self.host_queries.clone(),
            data,
        ))
    }

    /// Return all existing commits.
    pub fn commits(&self) -> Vec<[u8; 32]> {
        self.store.commits().into_iter().map(Into::into).collect()
    }

    /// Deletes the given commit from disk.
    pub fn delete_commit(&self, root: [u8; 32]) -> Result<(), Error> {
        self.store
            .delete_commit(root.into())
            .map_err(|err| PersistenceError(Arc::new(err)))
    }

    /// Return the root directory of the virtual machine.
    ///
    /// This is either the directory passed in by using [`new`], or the
    /// temporary directory created using [`ephemeral`].
    ///
    /// [`new`]: VM::new
    /// [`ephemeral`]: VM::ephemeral
    pub fn root_dir(&self) -> &Path {
        self.store.root_dir()
    }

    /// Returns a reference to the synchronization thread.
    pub fn sync_thread(&self) -> &thread::Thread {
        self.store.sync_loop()
    }
}

#[derive(Default, Clone)]
pub struct HostQueries {
    map: BTreeMap<Cow<'static, str>, Arc<dyn HostQuery>>,
}

impl Debug for HostQueries {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.map.keys()).finish()
    }
}

impl HostQueries {
    pub fn insert<Q, S>(&mut self, name: S, query: Q)
    where
        Q: 'static + HostQuery,
        S: Into<Cow<'static, str>>,
    {
        self.map.insert(name.into(), Arc::new(query));
    }

    pub fn call(
        &self,
        session: &mut Session,
        name: &str,
        buf: &mut [u8],
        len: u32,
    ) -> Option<u32> {
        self.map
            .get(name)
            .map(|host_query| host_query(session, buf, len))
    }
}

/// A query executable on the host.
///
/// The buffer containing the argument the contract used to call the query,
/// together with the argument's length, are passed as arguments to the
/// function, and should be processed first. Once this is done, the implementor
/// should emplace the return of the query in the same buffer, and return the
/// length written.
///
/// The host query will have access to the underlying session, and can use it to
/// perform calls to other contracts.
///
/// # Panics
/// If any error occurs during the execution, the implementer is encouraged to
/// signal this error by panicking.
pub trait HostQuery:
    Send + Sync + Fn(&mut Session, &mut [u8], u32) -> u32
{
}
impl<F> HostQuery for F where
    F: Send + Sync + Fn(&mut Session, &mut [u8], u32) -> u32
{
}
