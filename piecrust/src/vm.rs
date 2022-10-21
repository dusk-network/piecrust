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

use piecrust_uplink::ModuleId;

use crate::commit::{CommitId, ModuleCommitId, SessionCommit, SessionCommits};
use crate::memory_path::MemoryPath;
use crate::module::WrappedModule;
use crate::session::Session;
use crate::types::MemoryFreshness::*;
use crate::types::{MemoryFreshness, StandardBufSerializer};
use crate::util::{commit_id_to_name, module_id_to_name};
use crate::Error::{self, PersistenceError, RestoreError};

const SESSION_COMMITS_FILENAME: &str = "commits";

#[derive(Default)]
struct VMInner {
    modules: BTreeMap<ModuleId, WrappedModule>,
    host_queries: HostQueries,
    base_memory_path: PathBuf,
    session_commits: SessionCommits,
}

impl VMInner {
    fn new<P: AsRef<Path>>(path: P) -> Result<Self, Error>
    where
        P: Into<PathBuf>,
    {
        let base_memory_path = path.into();
        let session_commits = SessionCommits::from(
            base_memory_path.join(SESSION_COMMITS_FILENAME),
        )?;
        Ok(Self {
            modules: BTreeMap::default(),
            host_queries: HostQueries::default(),
            base_memory_path,
            session_commits,
        })
    }

    fn ephemeral() -> Result<Self, Error> {
        Ok(Self {
            modules: BTreeMap::default(),
            base_memory_path: tempdir()
                .map_err(PersistenceError)?
                .path()
                .into(),
            host_queries: HostQueries::default(),
            session_commits: SessionCommits::new(),
        })
    }
}

#[derive(Clone)]
pub struct VM {
    inner: Arc<RwLock<VMInner>>,
}

impl VM {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, Error>
    where
        P: Into<PathBuf>,
    {
        Ok(VM {
            inner: Arc::new(RwLock::new(VMInner::new(path)?)),
        })
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

    pub fn deploy(&mut self, bytecode: &[u8]) -> Result<ModuleId, Error> {
        let hash = blake3::hash(bytecode);
        let module_id = ModuleId::from(<[u8; 32]>::from(hash));
        self.deploy_with_id(module_id, bytecode)?;
        Ok(module_id)
    }

    pub fn deploy_with_id(
        &mut self,
        module_id: ModuleId,
        bytecode: &[u8],
    ) -> Result<(), Error> {
        // This should be the only place that we need a write lock.
        let mut guard = self.inner.write();
        let module = WrappedModule::new(bytecode)?;
        guard.modules.insert(module_id, module);
        Ok(())
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

    pub(crate) fn memory_path(
        &self,
        module_id: &ModuleId,
    ) -> (MemoryPath, MemoryFreshness) {
        (
            MemoryPath::new(
                self.inner
                    .read()
                    .base_memory_path
                    .join(module_id_to_name(*module_id)),
            ),
            Fresh,
        )
    }

    pub(crate) fn path_to_module_commit(
        &self,
        module_id: &ModuleId,
        module_commit_id: &ModuleCommitId,
    ) -> MemoryPath {
        const SEPARATOR: char = '_';
        let commit_id_name = &*commit_id_to_name(*module_commit_id);
        let mut name = module_id_to_name(*module_id);
        name.push(SEPARATOR);
        name.push_str(commit_id_name);
        let path = self.inner.read().base_memory_path.join(name);
        MemoryPath::new(path)
    }

    pub(crate) fn path_to_module_last_commit(
        &self,
        module_id: &ModuleId,
    ) -> MemoryPath {
        const LAST_COMMIT_POSTFIX: &str = "_last";
        let mut name = module_id_to_name(*module_id);
        name.push_str(LAST_COMMIT_POSTFIX);
        let path = self.inner.read().base_memory_path.join(name);
        MemoryPath::new(path)
    }

    fn path_to_session_commits(&self) -> PathBuf {
        self.inner
            .read()
            .base_memory_path
            .join(SESSION_COMMITS_FILENAME)
    }

    pub(crate) fn add_session_commit(&mut self, session_commit: SessionCommit) {
        self.inner.write().session_commits.add(session_commit);
    }

    pub(crate) fn restore_session(
        &self,
        session_commit_id: &CommitId,
    ) -> Result<(), Error> {
        self.inner.read().session_commits.with_every_module_commit(
            session_commit_id,
            |module_id, module_commit_id| {
                let source_path =
                    self.path_to_module_commit(module_id, module_commit_id);
                let (target_path, _) = self.memory_path(module_id);
                let last_commit_path =
                    self.path_to_module_last_commit(module_id);
                std::fs::copy(source_path.as_ref(), target_path.as_ref())
                    .map_err(RestoreError)?;
                std::fs::copy(source_path.as_ref(), last_commit_path.as_ref())
                    .map_err(RestoreError)?;
                Ok(())
            },
        )
    }

    pub fn persist(&self) -> Result<(), Error> {
        let guard = self.inner.read();
        guard
            .session_commits
            .persist(self.path_to_session_commits())
    }

    pub fn base_path(&self) -> PathBuf {
        self.inner.read().base_memory_path.to_path_buf()
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
