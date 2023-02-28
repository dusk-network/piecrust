// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::{self, Debug, Formatter};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

use tempfile::tempdir;

use crate::session::Session;
use crate::store::ModuleStore;
use crate::Error::{self, PersistenceError};

pub struct VM {
    host_queries: HostQueries,
    store: ModuleStore,
}

impl VM {
    /// Creates a new virtual machine, reading the given directory for existing
    /// commits and bytecode.
    pub fn new<P: AsRef<Path>>(dir: P) -> Result<Self, Error>
    where
        P: Into<PathBuf>,
    {
        let store = ModuleStore::new(dir)
            .map_err(|err| PersistenceError(Arc::new(err)))?;
        Ok(Self {
            host_queries: HostQueries::default(),
            store,
        })
    }

    /// Creates a new virtual machine using a temporary directory.
    pub fn ephemeral() -> Result<Self, Error> {
        let tmp = tempdir().map_err(|err| PersistenceError(Arc::new(err)))?;
        let tmp = tmp.path().to_path_buf();

        let store = ModuleStore::new(tmp)
            .map_err(|err| PersistenceError(Arc::new(err)))?;

        Ok(Self {
            host_queries: HostQueries::default(),
            store,
        })
    }

    /// Registers a [`HostQuery`] with the given `name`.
    pub fn register_host_query<Q, S>(&mut self, name: S, query: Q)
    where
        Q: 'static + HostQuery,
        S: Into<Cow<'static, str>>,
    {
        self.host_queries.insert(name, query);
    }

    pub fn session(&self, base: [u8; 32]) -> Result<Session, Error> {
        let module_session = self
            .store
            .session(base)
            .map_err(|err| PersistenceError(Arc::new(err)))?;
        Ok(Session::new(module_session, self.host_queries.clone()))
    }

    pub fn genesis_session(&self) -> Session {
        let module_session = self.store.genesis_session();
        Session::new(module_session, self.host_queries.clone())
    }

    /// Return all the existing commits on disk.
    pub fn commits(&self) -> Vec<[u8; 32]> {
        self.store.commits()
    }

    /// Remove the diff files from a commit by applying them to the base memory,
    /// and writing it back to disk.
    ///
    /// # Errors
    ///
    /// If this function fails, it may be due to any number of reasons:
    ///
    /// - [`remove_file`] may fail
    /// - [`write`] may fail
    ///
    /// Failing may result in a corrupted commit, and the user is encouraged to
    /// call [`delete_commit`].
    ///
    /// [`remove_file`]: fs::remove_file
    /// [`write`]: fs::write
    /// [`delete_commit`]: VM::delete_commit
    pub fn squash_commit(&self, root: [u8; 32]) -> Result<(), Error> {
        self.store
            .squash_commit(root)
            .map_err(|err| PersistenceError(Arc::new(err)))
    }

    /// Deletes the given commit from disk.
    pub fn delete_commit(&self, root: [u8; 32]) -> Result<(), Error> {
        self.store
            .delete_commit(root)
            .map_err(|err| PersistenceError(Arc::new(err)))
    }

    /// Return the root directory of the virtual machine.
    pub fn root_dir(&self) -> &Path {
        self.store.root_dir()
    }

    pub fn sync_loop_thread(&self) -> &thread::Thread {
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

    pub fn call(&self, name: &str, buf: &mut [u8], len: u32) -> Option<u32> {
        self.map.get(name).map(|host_query| host_query(buf, len))
    }
}

/// A query executable on the host.
///
/// The buffer containing the argument the module used to call the query
/// together with its length are passed as arguments to the function, and should
/// be processed first. Once this is done, the implementor should emplace the
/// return of the query in the same buffer, and return its length.
pub trait HostQuery: Send + Sync + Fn(&mut [u8], u32) -> u32 {}
impl<F> HostQuery for F where F: Send + Sync + Fn(&mut [u8], u32) -> u32 {}
