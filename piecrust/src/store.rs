// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! A library for dealing with memories in trees.

mod baseinfo;
mod bytecode;
mod commit;
mod commit_store;
mod hasher;
mod index;
mod memory;
mod metadata;
mod module;
mod session;
mod tree;
mod treepos;

use std::collections::BTreeMap;
use std::collections::btree_map::Entry::*;
use std::fmt::{Debug, Formatter};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::{fs, io, thread};

pub use bytecode::Bytecode;
use dusk_wasmtime::Engine;
pub use memory::{Memory, PAGE_SIZE};
pub use metadata::Metadata;
pub use module::Module;
use piecrust_uplink::ContractId;
use session::ContractDataEntry;
pub use session::ContractSession;
pub use tree::PageOpening;

use crate::store::commit::Commit;
use crate::store::commit::finalizer::CommitFinalizer;
use crate::store::commit::operation::CommitOperation;
use crate::store::commit::reader::CommitReader;
use crate::store::commit::remover::CommitRemover;
use crate::store::commit::writer::CommitWriter;
use crate::store::commit_store::CommitStore;
use crate::store::hasher::Hash;

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

        CommitOperation::recover_all(&self.root_dir)?;
        CommitWriter::recover_unpublished_roots(&self.root_dir)?;

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
    pub fn genesis_session(&self) -> io::Result<ContractSession> {
        self.call_with_replier(|replier| Call::StoreReady { replier })?;
        Ok(self.session_with_base(None))
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
            self.commit_store.lock().unwrap().get_commit(&hash).cloned()
        });
        ContractSession::new(
            &self.root_dir,
            self.engine.clone(),
            base_commit,
            self.call.as_ref().expect("call should exist").clone(),
            self.commit_store.clone(),
        )
    }

    /// Remove a compiled module file for a given contract.
    ///
    /// This removes the object code file from disk, which then
    /// needs recompilation when the contract is used again.
    pub fn remove_module(&self, contract_id: ContractId) -> io::Result<()> {
        CommitWriter::remove_module(&self.root_dir, contract_id)
    }

    /// Recompile a module from its bytecode.
    ///
    /// This reads the WASM bytecode from disk, recompiles it using the
    /// store's engine, and writes the compiled module back to disk.
    pub fn recompile_module(&self, contract_id: ContractId) -> io::Result<()> {
        CommitWriter::recompile_module(
            &self.root_dir,
            &self.engine,
            contract_id,
        )
    }
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
    StoreReady {
        replier: mpsc::SyncSender<io::Result<()>>,
    },
}

enum DeferredCommitOp {
    Delete(mpsc::SyncSender<io::Result<()>>),
    Finalize(mpsc::SyncSender<io::Result<()>>),
}

fn queue_deferred_op(
    deferred_ops: &mut BTreeMap<Hash, Vec<DeferredCommitOp>>,
    root: Hash,
    op: DeferredCommitOp,
) {
    match deferred_ops.entry(root) {
        Vacant(entry) => {
            entry.insert(vec![op]);
        }
        Occupied(mut entry) => {
            entry.get_mut().push(op);
        }
    }
}

fn delete_commit(
    root_dir: &Path,
    commit_store: &Arc<Mutex<CommitStore>>,
    root: Hash,
) -> io::Result<()> {
    let known = commit_store.lock().unwrap().contains_key(&root);
    if !known && CommitOperation::pending(root_dir, root)?.is_none() {
        return Ok(());
    }
    let io_result = CommitRemover::remove(root_dir, root);
    if io_result.is_ok() {
        commit_store.lock().unwrap().remove_commit(&root, false);
    }
    tracing::trace!("delete commit finished");
    io_result
}

fn finalize_commit(
    root_dir: &Path,
    commit_store: &Arc<Mutex<CommitStore>>,
    root: Hash,
) -> io::Result<()> {
    let commit = {
        let commit_store = commit_store.lock().unwrap();
        let Some(commit) = commit_store.get_commit(&root).cloned() else {
            tracing::trace!("finalizing commit finished");
            return Ok(());
        };
        commit
    };

    let io_result = CommitFinalizer::finalize(root, root_dir, &commit);
    match &io_result {
        Ok(_) => tracing::trace!(
            "finalizing commit proper finished: {:?}",
            hex::encode(root.as_bytes())
        ),
        Err(e) => tracing::trace!("finalizing commit proper failed {:?}", e),
    }
    if io_result.is_ok() {
        commit_store.lock().unwrap().remove_commit(&root, true);
    }
    tracing::trace!("finalizing commit finished");
    io_result
}

fn execute_deferred_op(
    root_dir: &Path,
    commit_store: &Arc<Mutex<CommitStore>>,
    root: Hash,
    op: DeferredCommitOp,
) -> io::Result<()> {
    match op {
        DeferredCommitOp::Delete(replier) => {
            let result = delete_commit(root_dir, commit_store, root);
            let return_result = result.as_ref().map(|_| ()).map_err(clone_io);
            let _ = replier.send(result);
            return_result
        }
        DeferredCommitOp::Finalize(replier) => {
            let result = finalize_commit(root_dir, commit_store, root);
            let return_result = result.as_ref().map(|_| ()).map_err(clone_io);
            let _ = replier.send(result);
            return_result
        }
    }
}

fn clone_io(error: &io::Error) -> io::Error {
    io::Error::new(error.kind(), error.to_string())
}

fn has_incomplete_operation(root_dir: &Path) -> bool {
    CommitOperation::any_pending(root_dir).unwrap_or(true)
}

fn operation_allowed(
    root_dir: &Path,
    root: Hash,
    incomplete_operation: bool,
) -> bool {
    !incomplete_operation
        || CommitOperation::pending(root_dir, root)
            .map(|pending| pending.is_some())
            .unwrap_or(false)
}

fn incomplete_operation_error() -> io::Error {
    io::Error::other("Commit store has an incomplete operation")
}

fn reject_deferred_op(op: DeferredCommitOp, cause: &io::Error) {
    let error = io::Error::new(
        io::ErrorKind::Interrupted,
        format!("Skipped after an earlier commit operation failed: {cause}"),
    );
    match op {
        DeferredCommitOp::Delete(replier)
        | DeferredCommitOp::Finalize(replier) => {
            let _ = replier.send(Err(error));
        }
    }
}

fn sync_loop<P: AsRef<Path>>(
    root_dir: P,
    commit_store: Arc<Mutex<CommitStore>>,
    calls: mpsc::Receiver<Call>,
) {
    let root_dir = root_dir.as_ref();

    let mut sessions = BTreeMap::new();

    let mut deferred_ops: BTreeMap<Hash, Vec<DeferredCommitOp>> =
        BTreeMap::new();
    let mut incomplete_operation = false;

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
                let io_result = if incomplete_operation {
                    Err(incomplete_operation_error())
                } else {
                    CommitWriter::create_and_write(
                        root_dir,
                        commit_store.clone(),
                        base,
                        contracts,
                    )
                };
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
                let commits = commit_store.lock().unwrap();
                let commits = if incomplete_operation {
                    commits
                        .keys()
                        .copied()
                        .filter(|root| {
                            CommitOperation::pending(root_dir, *root)
                                .map(|pending| pending.is_none())
                                .unwrap_or(false)
                        })
                        .collect()
                } else {
                    commits.keys().copied().collect()
                };
                let _ = replier.send(commits);
                tracing::trace!("get commits finished");
            }
            // Delete a commit from disk. If the commit is currently in use - as
            // in it is held by at least one session using `Call::CommitHold` -
            // queue it for deletion once no session is holding it.
            Call::CommitDelete {
                commit: root,
                replier,
            } => {
                tracing::trace!("delete commit started");
                if sessions.contains_key(&root) {
                    queue_deferred_op(
                        &mut deferred_ops,
                        root,
                        DeferredCommitOp::Delete(replier),
                    );

                    continue;
                }

                let result = if operation_allowed(
                    root_dir,
                    root,
                    incomplete_operation,
                ) {
                    delete_commit(root_dir, &commit_store, root)
                } else {
                    Err(incomplete_operation_error())
                };
                incomplete_operation = has_incomplete_operation(root_dir);
                let _ = replier.send(result);
            }
            // Finalize commit
            Call::CommitFinalize {
                commit: root,
                replier,
            } => {
                tracing::trace!("finalizing commit started");
                if sessions.contains_key(&root) {
                    queue_deferred_op(
                        &mut deferred_ops,
                        root,
                        DeferredCommitOp::Finalize(replier),
                    );

                    continue;
                }

                let result = if operation_allowed(
                    root_dir,
                    root,
                    incomplete_operation,
                ) {
                    finalize_commit(root_dir, &commit_store, root)
                } else {
                    Err(incomplete_operation_error())
                };
                incomplete_operation = has_incomplete_operation(root_dir);
                let _ = replier.send(result);
            }
            // Increment the hold count of a commit to prevent it from deletion
            // on a `Call::CommitDelete`.
            Call::CommitHold { base, replier } => {
                tracing::trace!("hold commit open session started");
                let mut maybe_base = None;
                if !incomplete_operation
                    && commit_store.lock().unwrap().contains_key(&base)
                {
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
            // `Call::CommitHold`. If this is the last session that held the
            // commit, execute queued delete and finalize operations.
            Call::SessionDrop(base) => {
                tracing::trace!("session drop started");
                match sessions.entry(base) {
                    Vacant(_) => unreachable!(
                        "If a session is dropped there must be a session hold entry"
                    ),
                    Occupied(mut entry) => {
                        *entry.get_mut() -= 1;

                        if *entry.get() == 0 {
                            entry.remove();

                            match deferred_ops.entry(base) {
                                Vacant(_) => {}
                                Occupied(entry) => {
                                    let mut failure = None;
                                    for op in entry.remove() {
                                        if !operation_allowed(
                                            root_dir,
                                            base,
                                            incomplete_operation,
                                        ) {
                                            reject_deferred_op(
                                                op,
                                                &incomplete_operation_error(),
                                            );
                                            continue;
                                        }
                                        if let Some(error) = &failure {
                                            reject_deferred_op(op, error);
                                            continue;
                                        }
                                        if let Err(error) = execute_deferred_op(
                                            root_dir,
                                            &commit_store,
                                            base,
                                            op,
                                        ) {
                                            failure = Some(error);
                                        }
                                    }
                                    incomplete_operation =
                                        has_incomplete_operation(root_dir);
                                }
                            }
                        }
                    }
                };
                tracing::trace!("session drop finished");
            }
            Call::StoreReady { replier } => {
                let result = if incomplete_operation {
                    Err(incomplete_operation_error())
                } else {
                    Ok(())
                };
                let _ = replier.send(result);
            }
        }
    }
}
