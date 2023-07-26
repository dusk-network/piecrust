// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! A library for dealing with memories in trees.

mod bytecode;
mod diff;
mod memory;
mod metadata;
mod objectcode;
mod session;
mod tree;

use std::collections::btree_map::Entry::*;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::{fs, io, thread};

use flate2::write::DeflateEncoder;
use flate2::Compression;

pub use bytecode::Bytecode;
use diff::diff;
pub use memory::Memory;
pub use metadata::Metadata;
pub use objectcode::Objectcode;
use piecrust_uplink::ContractId;
use session::ContractDataEntry;
pub use session::ContractSession;
use tree::{position_from_contract, Hash, Hasher, Tree};

const BYTECODE_DIR: &str = "bytecode";
const MEMORY_DIR: &str = "memory";
const DIFF_EXTENSION: &str = "diff";
const INDEX_FILE: &str = "index";
const TREE_FILE: &str = "merkle";
const OBJECTCODE_EXTENSION: &str = "a";
const METADATA_EXTENSION: &str = "m";

/// A store for all contract commits.
#[derive(Debug)]
pub struct ContractStore {
    sync_loop: thread::JoinHandle<()>,
    call: mpsc::Sender<Call>,
    root_dir: PathBuf,
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
    pub fn new<P: AsRef<Path>>(dir: P) -> io::Result<Self> {
        let root_dir = dir.as_ref();

        fs::create_dir_all(root_dir)?;

        let (call, calls) = mpsc::channel();
        let commits = read_all_commits(root_dir)?;

        let loop_root_dir = root_dir.to_path_buf();

        // The thread is given a name to allow for easily identifying it while
        // debugging.
        let sync_loop = thread::Builder::new()
            .name(String::from("PiecrustSync"))
            .spawn(|| sync_loop(loop_root_dir, commits, calls))?;

        Ok(Self {
            sync_loop,
            call,
            root_dir: root_dir.into(),
        })
    }

    /// Create a new [`ContractSession`] with the given `base` commit.
    ///
    /// Errors if the given base commit does not exist in the store.
    pub fn session(&self, base: Hash) -> io::Result<ContractSession> {
        let base_commit = self
            .call_with_replier(|replier| Call::CommitHold { base, replier })
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("No such base commit: {}", hex::encode(base)),
                )
            })?;

        Ok(self.session_with_base(Some(base_commit)))
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

    /// Remove the diff files from a commit by applying them to the base memory,
    /// and writing it back to disk.
    pub fn squash_commit(&self, commit: Hash) -> io::Result<()> {
        self.call_with_replier(|replier| Call::CommitSquash { commit, replier })
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("No such commit: {}", hex::encode(commit)),
                )
            })?
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

    /// Return the handle to the thread running the store's synchronization
    /// loop.
    pub fn sync_loop(&self) -> &thread::Thread {
        self.sync_loop.thread()
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

        self.call.send(closure(replier)).expect(
            "The receiver should never be dropped while there are senders",
        );

        receiver
            .recv()
            .expect("The sender should never be dropped without responding")
    }

    fn session_with_base(&self, base: Option<Commit>) -> ContractSession {
        ContractSession::new(&self.root_dir, base, self.call.clone())
    }
}

fn read_all_commits<P: AsRef<Path>>(
    root_dir: P,
) -> io::Result<BTreeMap<Hash, Commit>> {
    let root_dir = root_dir.as_ref();
    let mut commits = BTreeMap::new();

    for entry in fs::read_dir(root_dir)? {
        let entry = entry?;
        if entry.path().is_dir() {
            let commit = read_commit(entry.path())?;
            commits.insert(*commit.tree.root(), commit);
        }
    }

    Ok(commits)
}

fn read_commit<P: AsRef<Path>>(commit_dir: P) -> io::Result<Commit> {
    let commit_dir = commit_dir.as_ref();
    let commit = commit_from_dir(commit_dir)?;
    Ok(commit)
}

fn commit_from_dir<P: AsRef<Path>>(dir: P) -> io::Result<Commit> {
    let dir = dir.as_ref();

    let index_path = dir.join(INDEX_FILE);
    let tree_path = dir.join(TREE_FILE);

    let contracts = index_from_path(index_path)?;
    let tree = tree_from_path(tree_path)?;

    let mut diffs = BTreeSet::new();

    let bytecode_dir = dir.join(BYTECODE_DIR);
    let memory_dir = dir.join(MEMORY_DIR);

    for contract in contracts.keys() {
        let contract_hex = hex::encode(contract);

        // Check that all contracts in the index file have a corresponding
        // bytecode and memory.
        let bytecode_path = bytecode_dir.join(&contract_hex);
        if !bytecode_path.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Non-existing bytecode for contract: {contract_hex}"),
            ));
        }

        let memory_path = memory_dir.join(&contract_hex);
        if !memory_path.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Non-existing memory for contract: {contract_hex}"),
            ));
        }

        // If there is a diff file for a given contract, register it in the map.
        let diff_path = memory_path.with_extension(DIFF_EXTENSION);
        if diff_path.is_file() {
            diffs.insert(*contract);
        }
    }

    Ok(Commit {
        contracts,
        diffs,
        tree,
    })
}

fn index_from_path<P: AsRef<Path>>(
    path: P,
) -> io::Result<BTreeMap<ContractId, Hash>> {
    let path = path.as_ref();

    let contracts_bytes = fs::read(path)?;
    let contracts = rkyv::from_bytes(&contracts_bytes).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Invalid index file \"{path:?}\": {err}"),
        )
    })?;

    Ok(contracts)
}

fn tree_from_path<P: AsRef<Path>>(path: P) -> io::Result<Tree> {
    let path = path.as_ref();

    let tree_bytes = fs::read(path)?;
    let tree = rkyv::from_bytes(&tree_bytes).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Invalid merkle tree file \"{path:?}\": {err}"),
        )
    })?;

    Ok(tree)
}

#[derive(Debug, Clone)]
pub(crate) struct Commit {
    contracts: BTreeMap<ContractId, Hash>,
    diffs: BTreeSet<ContractId>,
    tree: Tree,
}

pub(crate) enum Call {
    Commit {
        contracts: BTreeMap<ContractId, ContractDataEntry>,
        base: Option<Commit>,
        replier: mpsc::SyncSender<io::Result<Commit>>,
    },
    GetCommits {
        replier: mpsc::SyncSender<Vec<Hash>>,
    },
    CommitDelete {
        commit: Hash,
        replier: mpsc::SyncSender<io::Result<()>>,
    },
    CommitSquash {
        commit: Hash,
        replier: mpsc::SyncSender<Option<io::Result<()>>>,
    },
    CommitHold {
        base: Hash,
        replier: mpsc::SyncSender<Option<Commit>>,
    },
    SessionDrop(Hash),
}

fn sync_loop<P: AsRef<Path>>(
    root_dir: P,
    commits: BTreeMap<Hash, Commit>,
    calls: mpsc::Receiver<Call>,
) {
    let root_dir = root_dir.as_ref();

    let mut sessions = BTreeMap::new();
    let mut commits = commits;

    let mut squash_bag = BTreeMap::new();
    let mut delete_bag = BTreeMap::new();

    for call in calls {
        match call {
            // Writes a session to disk and adds it to the map of existing commits.
            Call::Commit {
                contracts,
                base,
                replier,
            } => {
                let io_result = write_commit(root_dir, &mut commits, base, contracts);
                let _ = replier.send(io_result);
            }
            // Copy all commits and send them back to the caller.
            Call::GetCommits {
                replier
            } => {
                let _ = replier.send(commits.keys().copied().collect());
            }
            // Delete a commit from disk. If the commit is currently in use - as
            // in it is held by at least one session using `Call::SessionHold` -
            // queue it for deletion once no session is holding it.
            Call::CommitDelete { commit: root, replier } => {
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
                commits.remove(&root);
                let _ = replier.send(io_result);
            }
            // Squashing a commit on disk. If the commit is currently in use - as
            // in it is held by at least one session using `Call::SessionHold` -
            // queue it for squashing once no session is holding it.
            Call::CommitSquash {
                commit: root,
                replier,
            } => {
                match commits.get_mut(&root) {
                    None => {
                        let _ = replier.send(None);
                    }
                    Some(commit) => {
                        if sessions.contains_key(&root) {
                            match squash_bag.entry(root) {
                                Vacant(entry) => {
                                    entry.insert(vec![replier]);
                                }
                                Occupied(mut entry) => {
                                    entry.get_mut().push(replier);
                                }
                            }

                            continue;
                        }

                        let io_result = squash_commit(root_dir, root, commit);
                        commit.diffs.clear();
                        let _ = replier.send(Some(io_result));
                    }
                }
            }
            // Increment the hold count of a commit to prevent it from deletion
            // on a `Call::CommitDelete`.
            Call::CommitHold {
                base,
                replier,
            } => {
                let base_commit = commits.get(&base).cloned();

                if base_commit.is_some() {
                    match sessions.entry(base) {
                        Vacant(entry) => {
                            entry.insert(1);
                        }
                        Occupied(mut entry) => {
                            *entry.get_mut() += 1;
                        }
                    }
                }

                let _ = replier.send(base_commit);
            }
            // Signal that a session with a base commit has dropped and
            // decrements the hold count, once incremented using
            // `Call::SessionHold`. If this is the last session that held that
            // commit, and there are queued deletions, execute them.
            Call::SessionDrop(base) => match sessions.entry(base) {
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
                                    commits.remove(&base);
                                    let _ = replier.send(io_result);
                                }
                            }
                        }

                        // Try all squashes second
                        match squash_bag.entry(base) {
                            Vacant(_) => {}
                            Occupied(entry) => {
                                match commits.get_mut(&base) {
                                    None => {
                                        for replier in entry.remove() {
                                            let _ = replier.send(None);
                                        }
                                    }
                                    Some(commit) => {
                                        for replier in entry.remove() {
                                            let io_result = squash_commit(root_dir, base, commit);
                                            commit.diffs.clear();
                                            let _ = replier.send(Some(io_result));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
        }
    }
}

fn write_commit<P: AsRef<Path>>(
    root_dir: P,
    commits: &mut BTreeMap<Hash, Commit>,
    base: Option<Commit>,
    commit_contracts: BTreeMap<ContractId, ContractDataEntry>,
) -> io::Result<Commit> {
    let root_dir = root_dir.as_ref();
    let (contracts, tree) = compute_tree(&base, &commit_contracts);

    let root = *tree.root();
    let root_hex = hex::encode(root);
    let commit_dir = root_dir.join(root_hex);

    // Don't write the commit if it already exists on disk. This may happen if
    // the same transactions on the same base commit for example.
    if let Some(commit) = commits.get(&root) {
        return Ok(commit.clone());
    }

    match write_commit_inner(
        root_dir,
        &commit_dir,
        tree,
        base,
        contracts,
        commit_contracts,
    ) {
        Ok(commit) => {
            commits.insert(root, commit.clone());
            Ok(commit)
        }
        Err(err) => {
            let _ = fs::remove_dir_all(commit_dir);
            Err(err)
        }
    }
}

/// Writes a commit to disk.
fn write_commit_inner<P: AsRef<Path>>(
    root_dir: P,
    commit_dir: P,
    tree: Tree,
    base: Option<Commit>,
    contracts: BTreeMap<ContractId, Hash>,
    commit_contracts: BTreeMap<ContractId, ContractDataEntry>,
) -> io::Result<Commit> {
    let root_dir = root_dir.as_ref();
    let commit_dir = commit_dir.as_ref();

    let bytecode_dir = commit_dir.join(BYTECODE_DIR);
    fs::create_dir_all(&bytecode_dir)?;

    let memory_dir = commit_dir.join(MEMORY_DIR);
    fs::create_dir_all(&memory_dir)?;

    let mut diffs = BTreeSet::new();

    match base {
        None => {
            for (contract, store_data) in commit_contracts {
                let contract_hex = hex::encode(contract);

                let bytecode_path = bytecode_dir.join(&contract_hex);
                let objectcode_path =
                    bytecode_path.with_extension(OBJECTCODE_EXTENSION);
                let metadata_path =
                    bytecode_path.with_extension(METADATA_EXTENSION);
                let memory_path = memory_dir.join(&contract_hex);

                fs::write(bytecode_path, store_data.bytecode)?;
                fs::write(objectcode_path, store_data.objectcode)?;
                fs::write(metadata_path, store_data.metadata)?;
                fs::write(memory_path, &store_data.memory.read())?;
            }
        }
        Some(base_commit) => {
            let base = base_commit.tree.root();

            let base_hex = hex::encode(base);
            let base_dir = root_dir.join(base_hex);

            let base_bytecode_dir = base_dir.join(BYTECODE_DIR);
            let base_memory_dir = base_dir.join(MEMORY_DIR);

            for contract in base_commit.contracts.keys() {
                let contract_hex = hex::encode(contract);

                let bytecode_path = bytecode_dir.join(&contract_hex);
                let objectcode_path =
                    bytecode_path.with_extension(OBJECTCODE_EXTENSION);
                let metadata_path =
                    bytecode_path.with_extension(METADATA_EXTENSION);
                let memory_path = memory_dir.join(&contract_hex);

                let base_bytecode_path = base_bytecode_dir.join(&contract_hex);
                let base_objectcode_path =
                    base_bytecode_path.with_extension(OBJECTCODE_EXTENSION);
                let base_metadata_path =
                    base_bytecode_path.with_extension(METADATA_EXTENSION);
                let base_memory_path = base_memory_dir.join(&contract_hex);

                fs::hard_link(base_bytecode_path, bytecode_path)?;
                fs::hard_link(base_objectcode_path, objectcode_path)?;
                fs::hard_link(base_metadata_path, metadata_path)?;
                fs::hard_link(&base_memory_path, &memory_path)?;

                // If there is a diff of this memory in the base contract, and
                // it hasn't been touched in this commit, link
                // it as well.
                if base_commit.diffs.contains(contract)
                    && !commit_contracts.contains_key(contract)
                {
                    let base_diff_path =
                        base_memory_path.with_extension(DIFF_EXTENSION);
                    let diff_path = memory_path.with_extension(DIFF_EXTENSION);

                    fs::hard_link(base_diff_path, diff_path)?;
                    diffs.insert(*contract);
                }
            }

            for (contract, store_data) in commit_contracts {
                let contract_hex = hex::encode(contract);

                match base_commit.contracts.contains_key(&contract) {
                    true => {
                        let base_memory_path =
                            base_memory_dir.join(&contract_hex);
                        let memory_diff_path = memory_dir
                            .join(&contract_hex)
                            .with_extension(DIFF_EXTENSION);

                        let base_memory = Memory::from_file(base_memory_path)?;
                        let memory_diff = File::create(memory_diff_path)?;

                        let mut encoder = DeflateEncoder::new(
                            memory_diff,
                            Compression::default(),
                        );

                        diff(&base_memory, &store_data.memory, &mut encoder)?;

                        diffs.insert(contract);
                    }
                    false => {
                        let bytecode_path = bytecode_dir.join(&contract_hex);
                        let objectcode_path =
                            bytecode_path.with_extension(OBJECTCODE_EXTENSION);
                        let metadata_path =
                            bytecode_path.with_extension(METADATA_EXTENSION);
                        let memory_path = memory_dir.join(&contract_hex);

                        fs::write(bytecode_path, store_data.bytecode)?;
                        fs::write(objectcode_path, store_data.objectcode)?;
                        fs::write(metadata_path, store_data.metadata)?;
                        fs::write(memory_path, store_data.memory.read())?;
                    }
                }
            }
        }
    }

    let index_path = commit_dir.join(INDEX_FILE);
    let index_bytes = rkyv::to_bytes::<_, 128>(&contracts)
        .map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Failed serializing index file: {err}"),
            )
        })?
        .to_vec();
    fs::write(index_path, index_bytes)?;

    let merkle_path = commit_dir.join(TREE_FILE);
    let merkle_bytes = rkyv::to_bytes::<_, 128>(&tree)
        .map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Failed serializing merkle file: {err}"),
            )
        })?
        .to_vec();
    fs::write(merkle_path, merkle_bytes)?;

    Ok(Commit {
        contracts,
        diffs,
        tree,
    })
}

/// Delete the given commit's directory.
fn delete_commit_dir<P: AsRef<Path>>(
    root_dir: P,
    root: Hash,
) -> io::Result<()> {
    let root = hex::encode(root);
    let commit_dir = root_dir.as_ref().join(root);
    fs::remove_dir_all(commit_dir)
}

/// Squash the given commit.
fn squash_commit<P: AsRef<Path>>(
    root_dir: P,
    root: Hash,
    commit: &Commit,
) -> io::Result<()> {
    let root_dir = root_dir.as_ref();

    let root_hex = hex::encode(root);

    let commit_dir = root_dir.join(root_hex);

    let memory_dir = commit_dir.join(MEMORY_DIR);

    for contract in &commit.diffs {
        let contract_hex = hex::encode(contract);
        let memory_path = memory_dir.join(contract_hex);
        let memory_diff_path = memory_path.with_extension(DIFF_EXTENSION);

        let memory =
            Memory::from_file_and_diff(&memory_path, &memory_diff_path)?;

        fs::remove_file(&memory_path)?;
        fs::remove_file(memory_diff_path)?;

        fs::write(memory_path, memory.read())?;
    }

    Ok(())
}

/// Compute the new tree and contracts map resulting from the changed
/// `contracts`, and originating from the given `base` commit.
fn compute_tree<'a, I>(
    base: &Option<Commit>,
    contracts: I,
) -> (BTreeMap<ContractId, Hash>, Tree)
where
    I: IntoIterator<Item = (&'a ContractId, &'a ContractDataEntry)>,
{
    let iter = contracts.into_iter();

    let mut leaves_map = BTreeMap::new();
    let mut tree = match &base {
        Some(base) => base.tree.clone(),
        None => Tree::new(),
    };

    // Compute the hashes of changed memories
    for (contract, store_data) in iter {
        let mut hasher = Hasher::new();
        hasher.update(&store_data.memory.read());
        let hash = hasher.finalize();

        tree.insert(position_from_contract(contract), hash);
        leaves_map.insert(*contract, hash);
    }

    // Store the hashes of *un*changed memories
    if let Some(base_commit) = base {
        for (contract, hash) in &base_commit.contracts {
            if !leaves_map.contains_key(contract) {
                leaves_map.insert(*contract, *hash);
            }
        }
    }

    (leaves_map, tree)
}
