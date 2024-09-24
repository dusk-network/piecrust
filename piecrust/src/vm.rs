// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::any::Any;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::{self, Debug, Formatter};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

use dusk_wasmtime::{
    Config, Engine, ModuleVersionStrategy, OperatorCost, OptLevel, Strategy,
    WasmBacktraceDetails,
};
use tempfile::NamedTempFile;

use crate::config::BYTE_STORE_COST;
use crate::session::{Session, SessionData};
use crate::store::{ContractSession, StateStore, ZERO_HASH};
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

    const BYTE4_STORE_COST: i64 = 4 * BYTE_STORE_COST;
    const BYTE8_STORE_COST: i64 = 8 * BYTE_STORE_COST;
    const BYTE16_STORE_COST: i64 = 16 * BYTE_STORE_COST;

    config.operator_cost(OperatorCost {
        I32Store: BYTE4_STORE_COST,
        F32Store: BYTE4_STORE_COST,
        I32Store8: BYTE4_STORE_COST,
        I32Store16: BYTE4_STORE_COST,
        I32AtomicStore: BYTE4_STORE_COST,
        I32AtomicStore8: BYTE4_STORE_COST,
        I32AtomicStore16: BYTE4_STORE_COST,

        I64Store: BYTE8_STORE_COST,
        F64Store: BYTE8_STORE_COST,
        I64Store8: BYTE8_STORE_COST,
        I64Store16: BYTE8_STORE_COST,
        I64Store32: BYTE8_STORE_COST,
        I64AtomicStore: BYTE8_STORE_COST,
        I64AtomicStore8: BYTE8_STORE_COST,
        I64AtomicStore16: BYTE8_STORE_COST,
        I64AtomicStore32: BYTE8_STORE_COST,

        V128Store: BYTE16_STORE_COST,
        V128Store8Lane: BYTE16_STORE_COST,
        V128Store16Lane: BYTE16_STORE_COST,
        V128Store32Lane: BYTE16_STORE_COST,
        V128Store64Lane: BYTE16_STORE_COST,

        ..Default::default()
    });

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
    state_path: PathBuf,
    engine: Engine,
    host_queries: HostQueries,
}

impl Debug for VM {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("VM")
            .field("config", self.engine.config())
            .field("host_queries", &self.host_queries)
            .field("state_path", &self.state_path)
            .finish()
    }
}

impl VM {
    /// Creates a new [`VM`] using the state file at the given path.
    ///
    /// If there is no state file at the given path, one will be created, and
    /// populated with an empty state.
    ///
    /// # Errors
    /// If the state file cannot be opened, or the underlying database
    /// operations fail.
    pub fn new<P: Into<PathBuf>>(state_path: P) -> Result<Self, Error> {
        // We only need to open the state DB once to ensure that it is valid.
        // Any sessions will open a new connection.
        let state_path = state_path.into();
        let _ = StateStore::open(ZERO_HASH, &state_path).map_err(|err| {
            Error::PersistenceError(Arc::new(io::Error::other(err)))
        })?;

        let config = config();
        let engine = Engine::new(&config).expect(
            "Configuration should be valid since its set at compile time",
        );

        Ok(Self {
            state_path,
            engine,
            host_queries: HostQueries::default(),
        })
    }

    /// Creates a new `VM` using a new temporary file.
    ///
    /// Any session commits made by this machine should be considered discarded
    /// once this `VM` instance drops.
    ///
    /// # Errors
    /// If creating a temporary directory fails.
    pub fn ephemeral() -> Result<Self, Error> {
        let tmp_state_file = NamedTempFile::new()
            .map_err(|err| PersistenceError(Arc::new(err)))?;
        let tmp_state_path = tmp_state_file.into_temp_path();

        // we want to keep this temp file, since we can open it multiple times
        // in the same process
        let tmp_state_path = tmp_state_path
            .keep()
            .map_err(|err| PersistenceError(Arc::new(io::Error::other(err))))?;

        // We only need to open the state DB once to ensure that it is valid.
        // Any sessions will open a new connection.
        let state_path = tmp_state_path.to_path_buf();
        let _ = StateStore::open(ZERO_HASH, &state_path).map_err(|err| {
            Error::PersistenceError(Arc::new(io::Error::other(err)))
        })?;

        let config = config();
        let engine = Engine::new(&config).expect(
            "Configuration should be valid since its set at compile time",
        );

        Ok(Self {
            state_path,
            engine,
            host_queries: HostQueries::default(),
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
        base_root: Option<[u8; 32]>,
        data: impl Into<SessionData>,
    ) -> Result<Session, Error> {
        Session::new(
            &self.state_path,
            base_root.unwrap_or(ZERO_HASH),
            self.host_queries.clone(),
            data.into(),
            self.engine.clone(),
        )
    }

    /// Return all existing commits.
    pub fn commits(&self) -> Result<Vec<[u8; 32]>, Error> {
        StateStore::open(ZERO_HASH, &self.state_path)
            .and_then(|mut state| state.commits())
            .map_err(|err| {
                Error::PersistenceError(Arc::new(io::Error::other(err)))
            })
    }

    /// Deletes the given commit from disk.
    pub fn delete_commit(&self, root: [u8; 32]) -> Result<(), Error> {
        StateStore::open(ZERO_HASH, &self.state_path)
            .and_then(|mut state| state.delete_commit(root))
            .map_err(|err| {
                Error::PersistenceError(Arc::new(io::Error::other(err)))
            })
    }

    /// Return the path to the state of the virtual machine.
    ///
    /// This is either the path passed in by using [`new`], or the temporary
    /// directory created when using [`ephemeral`].
    ///
    /// [`new`]: VM::new
    /// [`ephemeral`]: VM::ephemeral
    pub fn state_path(&self) -> &Path {
        &self.state_path
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

    pub fn get(&self, name: &str) -> Option<&dyn HostQuery> {
        self.map.get(name).map(|q| q.as_ref())
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
/// Implementers of `Fn(&mut [u8], u32) -> u32` can be used as a `HostQuery`,
/// but the cost will be 0.
pub trait HostQuery: Send + Sync {
    /// Deserialize the argument buffer and return the price of the query.
    ///
    /// The buffer passed will be of the length of the argument the contract
    /// used to call the query.
    ///
    /// Any information needed to perform the query after deserializing the
    /// argument should be stored in `arg`, and will be passed to [`execute`],
    /// if there's enough gas to execute the query.
    ///
    /// [`execute`]: HostQuery::execute
    fn deserialize_and_price(
        &self,
        arg_buf: &[u8],
        arg: &mut Box<dyn Any>,
    ) -> u64;

    /// Perform the query and return the length of the result written to the
    /// argument buffer.
    ///
    /// The whole argument buffer is passed, together with any information
    /// stored in `arg` previously, during [`deserialize_and_price`].
    ///
    /// [`deserialize_and_price`]: HostQuery::deserialize_and_price
    fn execute(&self, arg: &Box<dyn Any>, arg_buf: &mut [u8]) -> u32;
}

/// An implementer of `Fn(&mut [u8], u32) -> u32` can be used as a `HostQuery`,
/// and the cost will be 0.
impl<F> HostQuery for F
where
    F: Send + Sync + Fn(&mut [u8], u32) -> u32,
{
    fn deserialize_and_price(
        &self,
        arg_buf: &[u8],
        arg: &mut Box<dyn Any>,
    ) -> u64 {
        let len = Box::new(arg_buf.len() as u32);
        *arg = len;
        0
    }

    fn execute(&self, arg: &Box<dyn Any>, arg_buf: &mut [u8]) -> u32 {
        let arg_len = *arg.downcast_ref::<u32>().unwrap();
        self(arg_buf, arg_len)
    }
}
