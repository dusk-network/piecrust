// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashSet};
use std::fmt::{self, Debug, Formatter};
use std::path::{Path, PathBuf};
use std::{fs, io};

use tempfile::tempdir;

use piecrust_uplink::{ModuleId, MODULE_ID_BYTES};

use crate::commit::{CommitId, ModuleCommitId, SessionCommit, SessionCommits};
use crate::memory_path::MemoryPath;
use crate::module::WrappedModule;
use crate::persistable::Persistable;
use crate::session::Session;
use crate::types::MemoryState;
use crate::util::{commit_id_to_name, module_id_to_name};
use crate::Error::{self, PersistenceError, RestoreError};

const SESSION_COMMITS_FILENAME: &str = "commits";
const LAST_COMMIT_POSTFIX: &str = "_last";
const LAST_COMMIT_ID_POSTFIX: &str = "_last_id";
const MODULES_DIR: &str = "modules";

/// Parse a module ID and the file from the given `path`.
///
/// # Panics
/// If the given path doesn't have a final component, or that final component is
/// not valid UTF-8.
fn module_from_path<P: AsRef<Path>>(
    path: P,
) -> Result<(ModuleId, WrappedModule), Error> {
    let path = path.as_ref();

    let fname = path
        .file_name()
        .expect("The path must have a final component")
        .to_str()
        .expect("The final path component should be valid UTF-8");

    let module_id_bytes = hex::decode(fname).ok().ok_or_else(|| {
        PersistenceError(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid hex in file name",
        ))
    })?;

    if module_id_bytes.len() != MODULE_ID_BYTES {
        return Err(PersistenceError(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "Expected file name of length {MODULE_ID_BYTES}, found {}",
                module_id_bytes.len()
            ),
        )));
    }

    let mut bytes = [0u8; MODULE_ID_BYTES];
    bytes.copy_from_slice(&module_id_bytes);

    let module_id = ModuleId::from_bytes(bytes);

    let bytecode = fs::read(path).map_err(PersistenceError)?;
    let module = WrappedModule::new(&bytecode)?;

    Ok((module_id, module))
}

fn read_modules<P: AsRef<Path>>(
    base_path: P,
) -> Result<BTreeMap<ModuleId, WrappedModule>, Error> {
    let modules_dir = base_path.as_ref().join(MODULES_DIR);
    let mut modules = BTreeMap::new();

    // If the directory doesn't exist, then there are no modules
    if !modules_dir.exists() {
        return Ok(modules);
    }

    for entry in fs::read_dir(modules_dir).map_err(PersistenceError)? {
        let entry = entry.map_err(PersistenceError)?;
        let entry_path = entry.path();

        // Only read if it is a file, otherwise simply ignore
        if entry_path.is_file() {
            let (module_id, module) = module_from_path(entry_path)?;
            modules.insert(module_id, module);
        }
    }

    Ok(modules)
}

pub struct VM {
    host_queries: HostQueries,
    base_memory_path: PathBuf,
    session_commits: SessionCommits,
    root: Option<[u8; 32]>,
    modules: BTreeMap<ModuleId, WrappedModule>,
}

impl VM {
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
        let path = self.base_memory_path.join(name);
        MemoryPath::new(path)
    }

    pub(crate) fn path_to_module_last_commit(
        &self,
        module_id: &ModuleId,
    ) -> MemoryPath {
        self.path_to_module_with_postfix(module_id, LAST_COMMIT_POSTFIX)
    }

    pub(crate) fn path_to_module_last_commit_id(
        &self,
        module_id: &ModuleId,
    ) -> MemoryPath {
        self.path_to_module_with_postfix(module_id, LAST_COMMIT_ID_POSTFIX)
    }

    fn path_to_module_with_postfix<P: AsRef<str>>(
        &self,
        module_id: &ModuleId,
        postfix: P,
    ) -> MemoryPath {
        let mut name = module_id_to_name(*module_id);
        name.push_str(postfix.as_ref());
        let path = self.base_memory_path.join(name);
        MemoryPath::new(path)
    }

    fn path_to_session_commits(&self) -> PathBuf {
        self.base_memory_path.join(SESSION_COMMITS_FILENAME)
    }

    pub(crate) fn add_session_commit(&mut self, session_commit: SessionCommit) {
        self.session_commits.add(session_commit);
    }

    pub(crate) fn restore_session(
        &mut self,
        session_commit_id: &CommitId,
    ) -> Result<(), Error> {
        self.reset_root();
        self.session_commits.with_every_module_commit(
            session_commit_id,
            |module_id, module_commit_id| {
                let source_path =
                    self.path_to_module_commit(module_id, module_commit_id);
                let (target_path, _) = self.memory_path(module_id);
                let last_commit_path =
                    self.path_to_module_last_commit(module_id);
                let last_commit_path_id =
                    self.path_to_module_last_commit_id(module_id);
                fs::copy(source_path.as_ref(), target_path.as_ref())
                    .map_err(RestoreError)?;
                fs::copy(source_path.as_ref(), last_commit_path.as_ref())
                    .map_err(RestoreError)?;
                module_commit_id.persist(last_commit_path_id)?;
                Ok(())
            },
        )
    }

    pub fn persist(&self) -> Result<(), Error> {
        self.session_commits.persist(self.path_to_session_commits())
    }

    pub fn base_path(&self) -> PathBuf {
        self.base_memory_path.to_path_buf()
    }

    pub(crate) fn get_current_vm_commit(&self) -> Result<SessionCommit, Error> {
        let mut module_ids: HashSet<ModuleId> = HashSet::new();
        let mut session_commit = SessionCommit::new();
        self.session_commits
            .with_every_session_commit(|session_commit| {
                for (module_id, _module_commit_id) in
                    session_commit.ids().iter()
                {
                    module_ids.insert(*module_id);
                }
            });
        for module_id in module_ids.iter() {
            let path = self.path_to_module_last_commit_id(module_id);
            if let Ok(module_commit_id) =
                ModuleCommitId::restore::<ModuleCommitId, MemoryPath>(path)
            {
                session_commit.add(module_id, &module_commit_id)
            }
        }
        session_commit.calculate_id();
        Ok(session_commit)
    }

    pub(crate) fn root(&mut self, refresh: bool) -> Result<[u8; 32], Error> {
        let current_root;
        {
            current_root = self.root;
        }
        match current_root {
            Some(r) if !refresh => Ok(r),
            _ => {
                let session_commit = self.get_current_vm_commit()?;
                let root = session_commit.commit_id().to_bytes();
                if refresh {
                    self.session_commits.add(session_commit);
                }
                self.root = Some(root);
                Ok(root)
            }
        }
    }

    pub(crate) fn reset_root(&mut self) {
        self.root = None;
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
