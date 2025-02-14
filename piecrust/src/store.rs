// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! A library for dealing with memories in trees.

mod bytecode;
mod commit;
mod commit_finalizer;
mod commit_hulk;
mod commit_reader;
mod commit_store;
mod commit_writer;
mod memory;
mod metadata;
mod module;
mod session;
mod tree;
mod treepos;

use std::collections::btree_map::Entry::*;
use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::{fs, io, thread};

use dusk_wasmtime::Engine;
use piecrust_uplink::ContractId;
use session::ContractDataEntry;
use tree::Hash;

use crate::store::commit::Commit;
use crate::store::commit_finalizer::CommitFinalizer;
use crate::store::commit_reader::CommitReader;
use crate::store::commit_store::CommitStore;
use crate::store::commit_writer::CommitWriter;
use crate::store::tree::BaseInfo;
pub use bytecode::Bytecode;
pub use memory::{Memory, PAGE_SIZE};
pub use metadata::Metadata;
pub use module::Module;
pub use session::ContractSession;
pub use tree::PageOpening;

const BYTECODE_DIR: &str = "bytecode";
const MEMORY_DIR: &str = "memory";
const LEAF_DIR: &str = "leaf";
const BASE_FILE: &str = "base";
const TREE_POS_FILE: &str = "tree_pos";
const TREE_POS_OPT_FILE: &str = "tree_pos_opt";
const ELEMENT_FILE: &str = "element";
const OBJECTCODE_EXTENSION: &str = "a";
const METADATA_EXTENSION: &str = "m";
const MAIN_DIR: &str = "main";

/// A store for all contract commits.
pub struct ContractStore {
    sync_loop: Option<thread::JoinHandle<()>>,
    engine: Engine,

    call: Option<mpsc::Sender<Call>>,
    root_dir: PathBuf,
    pub commit_store: Arc<Mutex<CommitStore>>,
}

impl Debug for ContractStore {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContractStore")
            .field("sync_loop", &self.sync_loop)
            .field("call", &self.call)
            .field("root_dir", &self.root_dir)
            .finish()
    }
}

impl ContractStore {
    /// Loads a new contract store from the given `dir`ectory.
    ///
    /// This also starts the synchronization loop, which is used to align
    /// [`commit`]s, [`delete`]s, and [`session spawning`] to avoid deleting
    /// commits in use by a session.
    ///
    /// [`commit`]: ContractSession::commit
    /// [`delete`]: ContractStore::delete_commit
    /// [`session spawning`]: ContractStore::session
    pub fn new<P: AsRef<Path>>(engine: Engine, dir: P) -> io::Result<Self> {
        let root_dir = dir.as_ref();

        fs::create_dir_all(root_dir)?;

        Ok(Self {
            sync_loop: None,
            engine,
            call: None,
            root_dir: root_dir.into(),
            commit_store: Arc::new(Mutex::new(CommitStore::new())),
        })
    }

    pub fn finish_new(&mut self) -> io::Result<()> {
        let loop_root_dir = self.root_dir.to_path_buf();
        let (call, calls) = mpsc::channel();
        let commit_store = self.commit_store.clone();

        tracing::trace!("before read_all_commit");
        CommitReader::read_all_commits(
            &self.engine,
            &self.root_dir,
            commit_store,
        )?;
        tracing::trace!("after read_all_commit");

        let commit_store = self.commit_store.clone();

        // The thread is given a name to allow for easily identifying it while
        // debugging.
        let sync_loop = thread::Builder::new()
            .name(String::from("PiecrustSync"))
            .spawn(|| sync_loop(loop_root_dir, commit_store, calls))?;

        self.sync_loop = Some(sync_loop);
        self.call = Some(call);
        Ok(())
    }

    /// Create a new [`ContractSession`] with the given `base` commit.
    ///
    /// Errors if the given base commit does not exist in the store.
    pub fn session(&self, base: Hash) -> io::Result<ContractSession> {
        tracing::trace!("session creation started");
        let base_commit_hash = self
            .call_with_replier(|replier| Call::CommitHold { base, replier })
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("No such base commit: {}", hex::encode(base)),
                )
            })?;

        let r = Ok(self.session_with_base(Some(base_commit_hash)));
        tracing::trace!("session creation finished");
        r
    }

    /// Create a new [`ContractSession`] that has no base commit.
    ///
    /// For session with a base commit, please see [`session`].
    ///
    /// [`session`]: ContractStore::session
    pub fn genesis_session(&self) -> ContractSession {
        self.session_with_base(None)
    }

    /// Returns the roots of the commits that are currently in the store.
    pub fn commits(&self) -> Vec<Hash> {
        self.call_with_replier(|replier| Call::GetCommits { replier })
    }

    /// Deletes a given `commit` from the store.
    ///
    /// If a `ContractSession` is currently using the given commit as a base,
    /// the operation will be queued for completion until the last session
    /// using the commit has dropped.
    ///
    /// It will block until the operation is completed.
    pub fn delete_commit(&self, commit: Hash) -> io::Result<()> {
        self.call_with_replier(|replier| Call::CommitDelete { commit, replier })
    }

    /// Finalizes commit
    ///
    /// The commit will become a "current" commit
    pub fn finalize_commit(&self, commit: Hash) -> io::Result<()> {
        self.call_with_replier(|replier| Call::CommitFinalize {
            commit,
            replier,
        })
    }

    /// Return the handle to the thread running the store's synchronization
    /// loop.
    pub fn sync_loop(&self) -> &thread::Thread {
        self.sync_loop
            .as_ref()
            .expect("sync thread should exist")
            .thread()
    }

    /// Return the path to the VM directory.
    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    fn call_with_replier<T, F>(&self, closure: F) -> T
    where
        F: FnOnce(mpsc::SyncSender<T>) -> Call,
    {
        let (replier, receiver) = mpsc::sync_channel(1);

        self.call
            .as_ref()
            .expect("call should exist")
            .send(closure(replier))
            .expect(
                "The receiver should never be dropped while there are senders",
            );

        receiver
            .recv()
            .expect("The sender should never be dropped without responding")
    }

    fn session_with_base(&self, base: Option<Hash>) -> ContractSession {
        let base_commit = base.and_then(|hash| {
            self.commit_store
                .lock()
                .unwrap()
                .get_commit(&hash)
                .map(|commit| commit.to_hulk())
        });
        ContractSession::new(
            &self.root_dir,
            self.engine.clone(),
            base_commit,
            self.call.as_ref().expect("call should exist").clone(),
        )
    }
}

fn base_from_path<P: AsRef<Path>>(path: P) -> io::Result<BaseInfo> {
    let path = path.as_ref();

    let base_info_bytes = fs::read(path)?;
    let base_info = rkyv::from_bytes(&base_info_bytes).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Invalid base info file \"{path:?}\": {err}"),
        )
    })?;

    Ok(base_info)
}

pub(crate) enum Call {
    Commit {
        contracts: BTreeMap<ContractId, ContractDataEntry>,
        base: Option<Commit>,
        replier: mpsc::SyncSender<io::Result<Hash>>,
    },
    GetCommits {
        replier: mpsc::SyncSender<Vec<Hash>>,
    },
    CommitDelete {
        commit: Hash,
        replier: mpsc::SyncSender<io::Result<()>>,
    },
    CommitFinalize {
        commit: Hash,
        replier: mpsc::SyncSender<io::Result<()>>,
    },
    CommitHold {
        base: Hash,
        replier: mpsc::SyncSender<Option<Hash>>,
    },
    SessionDrop(Hash),
}

fn sync_loop<P: AsRef<Path>>(
    root_dir: P,
    commit_store: Arc<Mutex<CommitStore>>,
    calls: mpsc::Receiver<Call>,
) {
    let root_dir = root_dir.as_ref();

    let mut sessions = BTreeMap::new();

    let mut delete_bag = BTreeMap::new();

    for call in calls {
        match call {
            // Writes a session to disk and adds it to the map of existing
            // commits.
            Call::Commit {
                contracts,
                base,
                replier,
            } => {
                tracing::trace!("writing commit started");
                let io_result = CommitWriter::write(
                    root_dir,
                    commit_store.clone(),
                    base,
                    contracts,
                );
                match &io_result {
                    Ok(hash) => tracing::trace!(
                        "writing commit finished: {:?}",
                        hex::encode(hash.as_bytes())
                    ),
                    Err(e) => tracing::trace!("writing commit failed {:?}", e),
                }
                let _ = replier.send(io_result);
            }
            // Copy all commits and send them back to the caller.
            Call::GetCommits { replier } => {
                tracing::trace!("get commits started");
                let _ = replier.send(
                    commit_store.lock().unwrap().keys().copied().collect(),
                );
                tracing::trace!("get commits finished");
            }
            // Delete a commit from disk. If the commit is currently in use - as
            // in it is held by at least one session using `Call::SessionHold` -
            // queue it for deletion once no session is holding it.
            Call::CommitDelete {
                commit: root,
                replier,
            } => {
                tracing::trace!("delete commit started");
                if sessions.contains_key(&root) {
                    match delete_bag.entry(root) {
                        Vacant(entry) => {
                            entry.insert(vec![replier]);
                        }
                        Occupied(mut entry) => {
                            entry.get_mut().push(replier);
                        }
                    }

                    continue;
                }

                let io_result = delete_commit_dir(root_dir, root);
                commit_store.lock().unwrap().remove_commit(&root);
                tracing::trace!("delete commit finished");
                let _ = replier.send(io_result);
            }
            // Finalize commit
            Call::CommitFinalize {
                commit: root,
                replier,
            } => {
                tracing::trace!("finalizing commit started");
                if sessions.contains_key(&root) {
                    match delete_bag.entry(root) {
                        Vacant(entry) => {
                            entry.insert(vec![replier]);
                        }
                        Occupied(mut entry) => {
                            entry.get_mut().push(replier);
                        }
                    }

                    continue;
                }

                let mut commit_store = commit_store.lock().unwrap();
                if let Some(_commit) = commit_store.get_commit(&root) {
                    tracing::trace!(
                        "finalizing commit proper started {}",
                        hex::encode(root.as_bytes())
                    );
                    let io_result = CommitFinalizer::finalize(root, root_dir);
                    match &io_result {
                        Ok(_) => tracing::trace!(
                            "finalizing commit proper finished: {:?}",
                            hex::encode(root.as_bytes())
                        ),
                        Err(e) => tracing::trace!(
                            "finalizing commit proper failed {:?}",
                            e
                        ),
                    }
                    commit_store.remove_commit(&root);
                    tracing::trace!("finalizing commit finished");
                    let _ = replier.send(io_result);
                } else {
                    tracing::trace!("finalizing commit finished");
                    let _ = replier.send(Ok(()));
                }
            }
            // Increment the hold count of a commit to prevent it from deletion
            // on a `Call::CommitDelete`.
            Call::CommitHold { base, replier } => {
                tracing::trace!("hold commit open session started");
                let mut maybe_base = None;
                if commit_store.lock().unwrap().contains_key(&base) {
                    maybe_base = Some(base);

                    match sessions.entry(base) {
                        Vacant(entry) => {
                            entry.insert(1);
                        }
                        Occupied(mut entry) => {
                            *entry.get_mut() += 1;
                        }
                    }
                }
                tracing::trace!("hold commit open session finished");

                let _ = replier.send(maybe_base);
            }
            // Signal that a session with a base commit has dropped and
            // decrements the hold count, once incremented using
            // `Call::SessionHold`. If this is the last session that held that
            // commit, and there are queued deletions, execute them.
            Call::SessionDrop(base) => {
                tracing::trace!("session drop started");
                match sessions.entry(base) {
                    Vacant(_) => unreachable!("If a session is dropped there must be a session hold entry"),
                    Occupied(mut entry) => {
                        *entry.get_mut() -= 1;

                        if *entry.get() == 0 {
                            entry.remove();

                            // Try all deletions first
                            match delete_bag.entry(base) {
                                Vacant(_) => {}
                                Occupied(entry) => {
                                    for replier in entry.remove() {
                                        let io_result =
                                            delete_commit_dir(root_dir, base);
                                        commit_store.lock().unwrap().remove_commit(&base);
                                        let _ = replier.send(io_result);
                                    }
                                }
                            }
                        }
                    }
                };
                tracing::trace!("session drop finished");
            }
        }
    }
}

/// Delete the given commit's directory.
fn delete_commit_dir<P: AsRef<Path>>(
    root_dir: P,
    root: Hash,
) -> io::Result<()> {
    let root = hex::encode(root);
    let root_main_dir = root_dir.as_ref().join(MAIN_DIR);
    let commit_dir = root_main_dir.join(&root);
    if commit_dir.exists() {
        let base_info_path = commit_dir.join(BASE_FILE);
        let base_info = base_from_path(base_info_path)?;
        for contract_hint in base_info.contract_hints {
            let contract_hex = hex::encode(contract_hint);
            let commit_mem_path = root_main_dir
                .join(MEMORY_DIR)
                .join(&contract_hex)
                .join(&root);
            fs::remove_dir_all(&commit_mem_path)?;
            let commit_leaf_path =
                root_main_dir.join(LEAF_DIR).join(&contract_hex).join(&root);
            fs::remove_dir_all(&commit_leaf_path)?;
        }
        fs::remove_dir_all(&commit_dir)?;
    }
    Ok(())
}
