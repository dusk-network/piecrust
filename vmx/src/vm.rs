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

use bytecheck::CheckBytes;
use parking_lot::RwLock;
use rkyv::{
    validation::validators::DefaultValidator, Archive, Deserialize, Infallible,
    Serialize,
};
use tempfile::tempdir;

use uplink::ModuleId;

use crate::module::WrappedModule;
use crate::session::{CommitId, Session};
use crate::types::MemoryFreshness::*;
use crate::types::{MemoryFreshness, StandardBufSerializer};
use crate::util::{commit_id_to_name, module_id_to_name};
use crate::Error::{self, PersistenceError};

#[derive(Debug)]
pub struct MemoryPath {
    path: PathBuf,
}

impl MemoryPath {
    pub fn new<P: AsRef<Path>>(path: P) -> Self
    where
        P: Into<PathBuf>,
    {
        MemoryPath { path: path.into() }
    }
}

impl AsRef<Path> for MemoryPath {
    fn as_ref(&self) -> &Path {
        self.path.as_path()
    }
}

#[derive(Default)]
struct VMInner {
    modules: BTreeMap<ModuleId, WrappedModule>,
    host_queries: HostQueries,
    base_memory_path: PathBuf,
    commit_ids: BTreeMap<ModuleId, CommitId>,
}

impl VMInner {
    fn new<P: AsRef<Path>>(path: P) -> Self
    where
        P: Into<PathBuf>,
    {
        Self {
            modules: BTreeMap::default(),
            host_queries: HostQueries::default(),
            base_memory_path: path.into(),
            commit_ids: BTreeMap::default(),
        }
    }

    fn ephemeral() -> Result<Self, Error> {
        Ok(Self {
            modules: BTreeMap::default(),
            base_memory_path: tempdir()
                .map_err(PersistenceError)?
                .path()
                .into(),
            host_queries: HostQueries::default(),
            commit_ids: BTreeMap::default(),
        })
    }
}

#[derive(Clone)]
pub struct VM {
    inner: Arc<RwLock<VMInner>>,
}

impl VM {
    pub fn new<P: AsRef<Path>>(path: P) -> Self
    where
        P: Into<PathBuf>,
    {
        VM {
            inner: Arc::new(RwLock::new(VMInner::new(path))),
        }
    }

    pub fn ephemeral() -> Result<Self, Error> {
        Ok(VM {
            inner: Arc::new(RwLock::new(VMInner::ephemeral()?)),
        })
    }

    /// Registers a [`HostQuery`] with the given `name`.
    pub fn register_host_query<Q, S>(&mut self, name: S, query: Q)
    where
        Q: 'static + HostQuery,
        S: Into<Cow<'static, str>>,
    {
        let mut guard = self.inner.write();
        guard.host_queries.insert(name, query);
    }

    pub fn module_memory_path(
        &self,
        module_id: &ModuleId,
    ) -> (MemoryPath, MemoryFreshness) {
        match self.memory_path_for_commit(module_id) {
            Some(path) => (path, NotFresh),
            None => {
                let path = MemoryPath::new(
                    self.inner
                        .read()
                        .base_memory_path
                        .join(module_id_to_name(*module_id)),
                );
                (path, Fresh)
            }
        }
    }

    fn memory_path_for_commit(
        &self,
        module_id: &ModuleId,
    ) -> Option<MemoryPath> {
        let guard = self.inner.read();
        let commit = guard.commit_ids.get(module_id);
        commit.map(|commit_id| {
            self.do_memory_path_for_commit(module_id, commit_id)
        })
    }

    fn do_memory_path_for_commit(
        &self,
        module_id: &ModuleId,
        commit_id: &CommitId,
    ) -> MemoryPath {
        let commit_id_name = &*commit_id_to_name(*commit_id);
        let mut name = module_id_to_name(*module_id);
        name.push('_');
        name.push_str(commit_id_name);
        let path = self.inner.read().base_memory_path.join(name);
        MemoryPath::new(path)
    }

    pub fn commit(
        &mut self,
        module_id: &ModuleId,
        commit_id: &CommitId,
    ) -> MemoryPath {
        self.inner.write().commit_ids.insert(*module_id, *commit_id);
        self.do_memory_path_for_commit(module_id, commit_id)
    }

    pub fn deploy(&mut self, bytecode: &[u8]) -> Result<ModuleId, Error> {
        // This should be the only place that we need a write lock.
        let mut guard = self.inner.write();
        let hash = blake3::hash(bytecode);
        let id = ModuleId::from(<[u8; 32]>::from(hash));

        let module = WrappedModule::new(bytecode)?;
        guard.modules.insert(id, module);
        Ok(id)
    }

    pub fn with_module<F, R>(&self, id: ModuleId, closure: F) -> R
    where
        F: FnOnce(&WrappedModule) -> R,
    {
        let guard = self.inner.read();
        let wrapped = guard.modules.get(&id).expect("invalid module");

        closure(wrapped)
    }

    pub fn query<Arg, Ret>(
        &self,
        id: ModuleId,
        method_name: &str,
        arg: Arg,
    ) -> Result<Ret, Error>
    where
        Arg: for<'b> Serialize<StandardBufSerializer<'b>>,
        Ret: Archive,
        Ret::Archived: Deserialize<Ret, Infallible>
            + for<'b> CheckBytes<DefaultValidator<'b>>,
    {
        let mut session = Session::new(self.clone());
        session.query(id, method_name, arg)
    }

    pub(crate) fn host_query(
        &self,
        name: &str,
        buf: &mut [u8],
        arg_len: u32,
    ) -> Option<u32> {
        let guard = self.inner.read();
        guard.host_queries.call(name, buf, arg_len)
    }

    pub fn session(&mut self) -> Session {
        Session::new(self.clone())
    }
}

#[derive(Default)]
pub struct HostQueries {
    map: BTreeMap<Cow<'static, str>, Box<dyn HostQuery>>,
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
        self.map.insert(name.into(), Box::new(query));
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
