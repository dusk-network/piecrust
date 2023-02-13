// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::{self, Debug, Formatter};
use std::fs;
use std::path::{Path, PathBuf};

use tempfile::tempdir;

use piecrust_uplink::ModuleId;

use crate::commit::{
    BagSizeInfo, CommitId, CommitPath, ModuleCommit, ModuleCommitBag,
    ModuleCommitId, ModuleCommitLike, SessionCommit, SessionCommits,
};
use crate::memory_path::MemoryPath;
use crate::merkle::Merkle;
use crate::module::WrappedModule;
use crate::persistable::Persistable;
use crate::session::Session;
use crate::types::MemoryState;
use crate::util::{module_id_to_name, read_modules};
use crate::Error::{self, PersistenceError, SessionError};

const SESSION_COMMITS_FILENAME: &str = "commits";
pub(crate) const MODULES_DIR: &str = "modules";

pub struct VM {
    host_queries: HostQueries,
    base_memory_path: PathBuf,
    session_commits: SessionCommits,
    root: Option<[u8; 32]>,
    modules: BTreeMap<ModuleId, WrappedModule>,
}

impl VM {
    /// Creates a new virtual machine, reading the given directory for existing
    /// commits and bytecode.
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, Error>
    where
        P: Into<PathBuf>,
    {
        let base_memory_path = path.into();
        let session_commits = SessionCommits::from(
            base_memory_path.join(SESSION_COMMITS_FILENAME),
        )?;
        let modules = read_modules(&base_memory_path)?;
        Ok(Self {
            host_queries: HostQueries::default(),
            base_memory_path,
            session_commits,
            root: None,
            modules,
        })
    }

    /// Creates a new virtual machine in using a temporary directory.
    pub fn ephemeral() -> Result<Self, Error> {
        Ok(Self {
            base_memory_path: tempdir()
                .map_err(PersistenceError)?
                .path()
                .into(),
            host_queries: HostQueries::default(),
            session_commits: SessionCommits::new(),
            root: None,
            modules: BTreeMap::default(),
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

    pub(crate) fn host_query(
        &self,
        name: &str,
        buf: &mut [u8],
        arg_len: u32,
    ) -> Option<u32> {
        self.host_queries.call(name, buf, arg_len)
    }

    pub fn session(&mut self) -> Session {
        Session::new(self)
    }
    pub(crate) fn get_module(&self, id: ModuleId) -> &WrappedModule {
        self.modules.get(&id).expect("invalid module")
    }
    pub(crate) fn insert_module(
        &mut self,
        module_id: ModuleId,
        bytecode: &[u8],
    ) -> Result<(), Error> {
        let modules_dir = self.base_memory_path.join(MODULES_DIR);
        let module = WrappedModule::new(bytecode)?;
        self.modules.insert(module_id, module);
        fs::create_dir_all(&modules_dir).map_err(PersistenceError)?;
        let module_hex = hex::encode(module_id.as_bytes());
        let module_path = modules_dir.join(module_hex);
        fs::write(module_path, bytecode).map_err(PersistenceError)?;
        Ok(())
    }

    pub(crate) fn memory_path(
        &self,
        module_id: &ModuleId,
    ) -> (MemoryPath, MemoryState) {
        Self::get_memory_path(&self.base_memory_path, module_id)
    }

    pub(crate) fn get_memory_path(
        base_path: &Path,
        module_id: &ModuleId,
    ) -> (MemoryPath, MemoryState) {
        (
            MemoryPath::new(base_path.join(module_id_to_name(*module_id))),
            MemoryState::Uninitialized,
        )
    }

    pub(crate) fn current_module_commit_path(
        &mut self,
        module_id: &ModuleId,
    ) -> Option<CommitPath> {
        let current_session_commit =
            self.session_commits.get_current_session_commit()?;
        let module_commit_id =
            current_session_commit.module_commit_ids().get(module_id)?;
        let (memory_path, _) = self.memory_path(module_id);
        let module_commit_id = *module_commit_id;
        self.get_bag_mut(module_id)
            .restore_module_commit(module_commit_id, &memory_path)
            .ok()?
    }

    fn path_to_session_commits(&self) -> PathBuf {
        self.base_memory_path.join(SESSION_COMMITS_FILENAME)
    }

    pub(crate) fn add_session_commit(
        &mut self,
        session_commit: SessionCommit,
    ) -> CommitId {
        self.session_commits.add_and_set_current(session_commit)
    }

    pub(crate) fn restore_session(
        &mut self,
        commit_id: &CommitId,
    ) -> Result<(), Error> {
        self.reset_root();
        let base_path = self.base_path();
        let mut pairs = Vec::<(ModuleId, ModuleCommitId)>::new();
        {
            let session_commit = self
                .session_commits
                .get_session_commit(commit_id)
                .ok_or_else(|| {
                    SessionError("unknown session commit id".into())
                })?;
            for (module_id, module_commit_id) in
                session_commit.module_commit_ids().iter()
            {
                pairs.push((*module_id, *module_commit_id))
            }
        }
        self.session_commits.set_current(commit_id);
        for (module_id, module_commit_id) in pairs {
            let (memory_path, _) =
                Self::get_memory_path(&base_path, &module_id);
            let restored = self
                .session_commits
                .get_bag_mut(&module_id)
                .restore_module_commit(module_commit_id, &memory_path)?;
            if let Some(commit_path) = restored {
                let commit = ModuleCommit::from_id_and_path_direct(
                    module_commit_id,
                    commit_path.path(),
                )?;
                commit.restore(&memory_path)?;
                if commit_path.can_remove() {
                    fs::remove_file(commit_path.path())
                        .map_err(PersistenceError)?;
                }
            }
        }
        Ok(())
    }

    pub fn persist(&self) -> Result<(), Error> {
        self.session_commits.persist(self.path_to_session_commits())
    }

    pub fn base_path(&self) -> PathBuf {
        self.base_memory_path.to_path_buf()
    }

    pub(crate) fn compute_current_root(&self) -> Result<[u8; 32], Error> {
        let mut vec = Vec::new();
        if let Some(current_session_commit) =
            self.session_commits.get_current_session_commit()
        {
            for (_module_id, module_commit_id) in
                current_session_commit.module_commit_ids().iter()
            {
                vec.push(*module_commit_id)
            }
        }
        vec.sort();
        Ok(Merkle::merkle(&mut vec).to_bytes())
    }

    pub(crate) fn root(&mut self, refresh: bool) -> Result<[u8; 32], Error> {
        let current_root;
        {
            current_root = self.root;
        }
        match current_root {
            Some(r) if !refresh => Ok(r),
            _ => {
                self.set_root(self.compute_current_root()?);
                Ok(self.root.expect("root should exist"))
            }
        }
    }

    pub(crate) fn reset_root(&mut self) {
        self.root = None;
    }

    pub(crate) fn set_root(&mut self, root: [u8; 32]) {
        self.root = Some(root);
    }

    pub(crate) fn get_bag_mut(
        &mut self,
        module_id: &ModuleId,
    ) -> &mut ModuleCommitBag {
        self.session_commits.get_bag_mut(module_id)
    }

    pub(crate) fn get_bag(
        &self,
        module_id: &ModuleId,
    ) -> Option<&ModuleCommitBag> {
        self.session_commits.get_bag(module_id)
    }

    pub(crate) fn get_bag_size_info(
        &self,
        module_id: &ModuleId,
    ) -> Result<BagSizeInfo, Error> {
        let (memory_path, _) = self.memory_path(module_id);
        self.get_bag(module_id)
            .expect("module bag found")
            .get_bag_size_info(&memory_path)
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
