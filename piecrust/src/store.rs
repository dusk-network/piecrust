// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! A library for dealing with memories in trees.

mod bytecode;
mod memory;
mod metadata;
mod module;
mod session;
mod tree;

use std::cell::Ref;
use std::collections::btree_map::Entry::*;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Debug, Formatter};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::{fs, io, thread};

use dusk_wasmtime::Engine;
use piecrust_uplink::ContractId;
use session::ContractDataEntry;
use tree::{Hash, NewContractIndex};

use crate::store::tree::{
    position_from_contract, BaseInfo, ContractIndexElement, ContractsMerkle,
    PageTree,
};
pub use bytecode::Bytecode;
pub use memory::{Memory, PAGE_SIZE};
pub use metadata::Metadata;
pub use module::Module;
pub use session::ContractSession;
pub use tree::PageOpening;

const BYTECODE_DIR: &str = "bytecode";
const MEMORY_DIR: &str = "memory";
const INDEX_FILE: &str = "index";
const MERKLE_FILE: &str = "merkle";
const BASE_FILE: &str = "base";
const OBJECTCODE_EXTENSION: &str = "a";
const METADATA_EXTENSION: &str = "m";
const MAIN_DIR: &str = "main";

/// A store for all contract commits.
pub struct ContractStore {
    sync_loop: thread::JoinHandle<()>,
    engine: Engine,

    call: mpsc::Sender<Call>,
    root_dir: PathBuf,
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

        let (call, calls) = mpsc::channel();
        let commits = read_all_commits(&engine, root_dir)?;

        // here is a place where central objects should be read from disk
        // central objects are:
        //     1) merkle tree
        //     2) contracts map (ContractId, Option<CommitId>) ->
        //        ContractIndexElement

        let loop_root_dir = root_dir.to_path_buf();

        // The thread is given a name to allow for easily identifying it while
        // debugging.
        let sync_loop = thread::Builder::new()
            .name(String::from("PiecrustSync"))
            .spawn(|| sync_loop(loop_root_dir, commits, calls))?;

        Ok(Self {
            sync_loop,
            engine,
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
        ContractSession::new(
            &self.root_dir,
            self.engine.clone(),
            base,
            self.call.clone(),
        )
    }
}

fn read_all_commits<P: AsRef<Path>>(
    engine: &Engine,
    root_dir: P,
) -> io::Result<BTreeMap<Hash, Commit>> {
    let root_dir = root_dir.as_ref();
    let mut commits = BTreeMap::new();

    let root_dir = root_dir.join(MAIN_DIR);
    fs::create_dir_all(root_dir.clone())?;

    if root_dir.join(INDEX_FILE).is_file() {
        let commit = read_commit(engine, root_dir.clone())?;
        let root = *commit.root();
        commits.insert(root, commit);
    }

    for entry in fs::read_dir(root_dir)? {
        let entry = entry?;
        if entry.path().is_dir() {
            let filename = entry.file_name().to_string_lossy().to_string();
            if filename == MEMORY_DIR || filename == BYTECODE_DIR {
                continue;
            }
            let commit = read_commit(engine, entry.path())?;
            let root = *commit.root();
            commits.insert(root, commit);
        }
    }

    Ok(commits)
}

fn read_commit<P: AsRef<Path>>(
    engine: &Engine,
    commit_dir: P,
) -> io::Result<Commit> {
    let commit_dir = commit_dir.as_ref();
    let commit = commit_from_dir(engine, commit_dir)?;
    Ok(commit)
}

fn page_path<P: AsRef<Path>>(memory_dir: P, page_index: usize) -> PathBuf {
    memory_dir.as_ref().join(format!("{page_index}"))
}

fn page_path_main<P: AsRef<Path>, S: AsRef<str>>(
    memory_dir: P,
    page_index: usize,
    commit_id: S,
) -> io::Result<PathBuf> {
    let commit_id = commit_id.as_ref();
    let dir = memory_dir.as_ref().join(commit_id);
    fs::create_dir_all(&dir)?;
    Ok(dir.join(format!("{page_index}")))
}

fn index_path_main<P: AsRef<Path>, S: AsRef<str>>(
    main_dir: P,
    commit_id: S,
) -> io::Result<PathBuf> {
    let commit_id = commit_id.as_ref();
    let dir = main_dir.as_ref().join(commit_id);
    fs::create_dir_all(&dir)?;
    Ok(dir.join(INDEX_FILE))
}

fn merkle_path_main<P: AsRef<Path>, S: AsRef<str>>(
    main_dir: P,
    commit_id: S,
) -> io::Result<PathBuf> {
    let commit_id = commit_id.as_ref();
    let dir = main_dir.as_ref().join(commit_id);
    fs::create_dir_all(&dir)?;
    Ok(dir.join(MERKLE_FILE))
}

fn base_path_main<P: AsRef<Path>, S: AsRef<str>>(
    main_dir: P,
    commit_id: S,
) -> io::Result<PathBuf> {
    let commit_id = commit_id.as_ref();
    let dir = main_dir.as_ref().join(commit_id);
    fs::create_dir_all(&dir)?;
    Ok(dir.join(BASE_FILE))
}

fn commit_id_to_hash<S: AsRef<str>>(commit_id: S) -> Hash {
    let hash: [u8; 32] = hex::decode(commit_id.as_ref())
        .expect("Hex decoding of commit id string should succeed")
        .try_into()
        .expect("Commit id string conversion should succeed");
    Hash::from(hash)
}

fn commit_from_dir<P: AsRef<Path>>(
    engine: &Engine,
    dir: P,
) -> io::Result<Commit> {
    let dir = dir.as_ref();
    let mut commit_id: Option<String> = None;
    let main_dir = if dir
        .file_name()
        .expect("Filename or folder name should exist")
        != MAIN_DIR
    {
        commit_id = Some(
            dir.file_name()
                .expect("Filename or folder name should exist")
                .to_string_lossy()
                .to_string(),
        );
        // this means we are in a commit dir, need to back up for bytecode
        // and memory paths to work correctly
        dir.parent().expect("Parent should exist")
    } else {
        dir
    };
    let maybe_hash = commit_id.as_ref().map(commit_id_to_hash);

    let index_path = dir.join(INDEX_FILE);
    let contracts_merkle_path = dir.join(MERKLE_FILE);
    let (index, contracts_merkle) =
        index_merkle_from_path(index_path, contracts_merkle_path)?;

    let bytecode_dir = main_dir.join(BYTECODE_DIR);
    let memory_dir = main_dir.join(MEMORY_DIR);

    for (contract, contract_index) in index.iter() {
        let contract_hex = hex::encode(contract);

        // Check that all contracts in the index file have a corresponding
        // bytecode and memory pages specified.
        let bytecode_path = bytecode_dir.join(&contract_hex);
        if !bytecode_path.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Non-existing bytecode for contract: {contract_hex}"),
            ));
        }

        let module_path = bytecode_path.with_extension(OBJECTCODE_EXTENSION);

        // SAFETY it is safe to deserialize the file here, since we don't use
        // the module here. We just want to check if the file is valid.
        if Module::from_file(engine, &module_path).is_err() {
            let bytecode = Bytecode::from_file(bytecode_path)?;
            let module = Module::from_bytecode(engine, bytecode.as_ref())
                .map_err(|err| {
                    io::Error::new(io::ErrorKind::InvalidData, err)
                })?;
            fs::write(module_path, module.serialize())?;
        }

        let contract_memory_dir = memory_dir.join(&contract_hex);

        for page_index in &contract_index.page_indices {
            let main_page_path = page_path(&contract_memory_dir, *page_index);
            if !main_page_path.is_file() {
                let path = ContractSession::find_page(
                    *page_index,
                    maybe_hash,
                    contract_memory_dir.clone(),
                    main_dir,
                );
                let found = path.map(|p| p.is_file()).unwrap_or(false);
                if !found {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "Non-existing memory for contract: {contract_hex}"
                        ),
                    ));
                }
            }
        }
    }

    Ok(Commit {
        index,
        contracts_merkle,
        maybe_hash,
    })
}

fn index_merkle_from_path<P1: AsRef<Path>, P2: AsRef<Path>>(
    path: P1,
    merkle_path: P2,
) -> io::Result<(NewContractIndex, ContractsMerkle)> {
    let path = path.as_ref();
    let merkle_path = merkle_path.as_ref();

    tracing::trace!("reading index file started");
    let index_bytes = fs::read(path)?;
    tracing::trace!("reading index file finished");

    tracing::trace!("deserializing index file started");
    let index = rkyv::from_bytes(&index_bytes).map_err(|err| {
        tracing::trace!("deserializing index file failed {}", err);
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Invalid index file \"{path:?}\": {err}"),
        )
    })?;
    tracing::trace!("deserializing index file finished");

    tracing::trace!("reading contracts merkle file started");
    let merkle_bytes = fs::read(merkle_path)?;
    tracing::trace!("reading contracts merkle file finished");

    tracing::trace!("deserializing contracts merkle file started");
    let merkle = rkyv::from_bytes(&merkle_bytes).map_err(|err| {
        tracing::trace!("deserializing contracts merkle file failed {}", err);
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Invalid contracts merkle file \"{merkle_path:?}\": {err}"),
        )
    })?;
    tracing::trace!("deserializing contracts merkle file finished");

    Ok((index, merkle))
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

#[derive(Debug, Default, Clone)]
pub(crate) struct Commit {
    index: NewContractIndex,
    contracts_merkle: ContractsMerkle,
    maybe_hash: Option<Hash>,
}

impl Commit {
    pub fn inclusion_proofs(
        mut self,
        contract_id: &ContractId,
        _maybe_commit_id: Option<Hash>,
    ) -> Option<impl Iterator<Item = (usize, PageOpening)>> {
        let contract = self.index.contracts.remove(contract_id)?;

        let pos = position_from_contract(contract_id);

        Some(contract.page_indices.into_iter().map(move |page_index| {
            let tree_opening = self
                .contracts_merkle
                .tree
                .opening(pos)
                .expect("There must be a leaf for the contract");

            let page_opening = contract
                .tree
                .opening(page_index as u64)
                .expect("There must be a leaf for the page");

            (
                page_index,
                PageOpening {
                    tree: tree_opening,
                    inner: page_opening,
                },
            )
        }))
    }

    pub fn insert(&mut self, contract: ContractId, memory: &Memory) {
        if self.index.contracts.get(&contract).is_none() {
            self.index.contracts.insert(
                contract,
                ContractIndexElement {
                    tree: PageTree::new(memory.is_64()),
                    len: 0,
                    page_indices: BTreeSet::new(),
                },
            );
        }
        let element = self.index.contracts.get_mut(&contract).unwrap();

        element.len = memory.current_len;

        for (dirty_page, _, page_index) in memory.dirty_pages() {
            element.page_indices.insert(*page_index);
            let hash = Hash::new(dirty_page);
            element.tree.insert(*page_index as u64, hash);
        }

        self.contracts_merkle
            .tree
            .insert(position_from_contract(&contract), *element.tree.root());
    }

    pub fn remove_and_insert(&mut self, contract: ContractId, memory: &Memory) {
        self.index.contracts.remove(&contract);
        self.insert(contract, memory);
    }

    pub fn root(&self) -> Ref<Hash> {
        self.contracts_merkle.tree.root()
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
                let io_result =
                    write_commit(root_dir, &mut commits, base, contracts);
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
                let _ = replier.send(commits.keys().copied().collect());
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
                commits.remove(&root);
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

                if let Some(commit) = commits.get(&root).cloned() {
                    tracing::trace!(
                        "finalizing commit proper started {}",
                        hex::encode(root.as_bytes())
                    );
                    let io_result = finalize_commit(root, root_dir, &commit);
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
                    commits.remove(&root);
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
                let base_commit = commits.get(&base).cloned();
                tracing::trace!("hold commit getting commit finished");

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
                tracing::trace!("hold commit open session finished");

                let _ = replier.send(base_commit);
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
                                        commits.remove(&base);
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

fn write_commit<P: AsRef<Path>>(
    root_dir: P,
    commits: &mut BTreeMap<Hash, Commit>,
    base: Option<Commit>,
    commit_contracts: BTreeMap<ContractId, ContractDataEntry>,
) -> io::Result<Hash> {
    let root_dir = root_dir.as_ref();

    let index = base
        .as_ref()
        .map_or(NewContractIndex::default(), |base| base.index.clone());
    let contracts_merkle =
        base.as_ref().map_or(ContractsMerkle::default(), |base| {
            base.contracts_merkle.clone()
        });
    let mut commit = Commit {
        index,
        contracts_merkle,
        maybe_hash: base.as_ref().map_or(None, |base| base.maybe_hash),
    };

    for (contract_id, contract_data) in &commit_contracts {
        if contract_data.is_new {
            commit.remove_and_insert(*contract_id, &contract_data.memory);
        } else {
            commit.insert(*contract_id, &contract_data.memory);
        }
    }

    tracing::trace!("calculating root started");
    let root = *commit.root();
    let root_hex = hex::encode(root);
    tracing::trace!("calculating root finished");

    // Don't write the commit if it already exists on disk. This may happen if
    // the same transactions on the same base commit for example.
    if commits.contains_key(&root) {
        return Ok(root);
    }

    write_commit_inner(root_dir, &commit, commit_contracts, root_hex, base).map(
        |_| {
            commits.insert(root, commit);
            root
        },
    )
}

/// Writes a commit to disk.
fn write_commit_inner<P: AsRef<Path>, S: AsRef<str>>(
    root_dir: P,
    commit: &Commit,
    commit_contracts: BTreeMap<ContractId, ContractDataEntry>,
    commit_id: S,
    maybe_base: Option<Commit>,
) -> io::Result<()> {
    let root_dir = root_dir.as_ref();
    let mut base_info = BaseInfo {
        maybe_base: maybe_base.map(|base| *base.root()),
        ..Default::default()
    };

    struct Directories {
        main_dir: PathBuf,
        bytecode_main_dir: PathBuf,
        memory_main_dir: PathBuf,
    }

    let directories = {
        let main_dir = root_dir.join(MAIN_DIR);
        fs::create_dir_all(&main_dir)?;

        let bytecode_main_dir = main_dir.join(BYTECODE_DIR);
        fs::create_dir_all(&bytecode_main_dir)?;

        let memory_main_dir = main_dir.join(MEMORY_DIR);
        fs::create_dir_all(&memory_main_dir)?;

        Directories {
            main_dir,
            bytecode_main_dir,
            memory_main_dir,
        }
    };

    // Write the dirty pages contracts of contracts to disk.
    for (contract, contract_data) in &commit_contracts {
        let contract_hex = hex::encode(contract);

        let memory_main_dir = directories.memory_main_dir.join(&contract_hex);

        fs::create_dir_all(&memory_main_dir)?;

        let mut pages = BTreeSet::new();

        let mut dirty = false;
        // Write dirty pages and keep track of the page indices.
        for (dirty_page, _, page_index) in contract_data.memory.dirty_pages() {
            let page_path: PathBuf = page_path_main(
                &memory_main_dir,
                *page_index,
                commit_id.as_ref(),
            )?;
            fs::write(page_path.clone(), dirty_page)?;
            pages.insert(*page_index);
            dirty = true;
        }

        let bytecode_main_path =
            directories.bytecode_main_dir.join(&contract_hex);
        let module_main_path =
            bytecode_main_path.with_extension(OBJECTCODE_EXTENSION);
        let metadata_main_path =
            bytecode_main_path.with_extension(METADATA_EXTENSION);

        // If the contract is new, we write the bytecode, module, and metadata
        // files to disk.
        if contract_data.is_new {
            // we write them to the main location
            fs::write(bytecode_main_path, &contract_data.bytecode)?;
            fs::write(module_main_path, &contract_data.module.serialize())?;
            fs::write(metadata_main_path, &contract_data.metadata)?;
            dirty = true;
        }
        if dirty {
            base_info.contract_hints.push(*contract);
        }
    }

    let index_main_path =
        index_path_main(directories.main_dir.clone(), commit_id.as_ref())?;
    let merkle_main_path =
        merkle_path_main(directories.main_dir.clone(), commit_id.as_ref())?;

    tracing::trace!("serializing index started");
    let index_bytes =
        rkyv::to_bytes::<_, 128>(&commit.index).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Failed serializing index file: {err}"),
            )
        })?;
    tracing::trace!("serializing index finished");
    tracing::trace!("writing index file started");
    fs::write(index_main_path.clone(), index_bytes)?;
    tracing::trace!("writing index file finished");

    tracing::trace!("serializing contracts merkle file started");
    let merkle_bytes = rkyv::to_bytes::<_, 128>(&commit.contracts_merkle)
        .map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Failed serializing contracts merkle file file: {err}"),
            )
        })?;
    tracing::trace!("serializing contracts merkle file finished");
    tracing::trace!("writing contracts merkle file started");
    fs::write(merkle_main_path.clone(), merkle_bytes)?;
    tracing::trace!("writing contracts merkle file finished");

    let base_main_path =
        base_path_main(directories.main_dir, commit_id.as_ref())?;
    let base_info_bytes =
        rkyv::to_bytes::<_, 128>(&base_info).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Failed serializing base info file: {err}"),
            )
        })?;
    fs::write(base_main_path.clone(), base_info_bytes)?;

    Ok(())
}

/// Delete the given commit's directory.
fn delete_commit_dir<P: AsRef<Path>>(
    root_dir: P,
    root: Hash,
) -> io::Result<()> {
    let root = hex::encode(root);
    let root_main_dir = root_dir.as_ref().join(MAIN_DIR);
    let commit_dir = root_main_dir.join(root.clone());
    if commit_dir.exists() {
        let base_info_path = commit_dir.join(BASE_FILE);
        let base_info = base_from_path(base_info_path.clone())?;
        for contract_hint in base_info.contract_hints {
            let contract_hex = hex::encode(contract_hint);
            let commit_mem_path = root_main_dir
                .join(MEMORY_DIR)
                .join(contract_hex.clone())
                .join(root.clone());
            fs::remove_dir_all(commit_mem_path.clone())?;
        }
        fs::remove_dir_all(commit_dir.clone())?;
    }
    Ok(())
}

/// Finalize commit
fn finalize_commit<P: AsRef<Path>>(
    root: Hash,
    root_dir: P,
    _commit: &Commit,
) -> io::Result<()> {
    let main_dir = root_dir.as_ref().join(MAIN_DIR);
    let root = hex::encode(root);
    let commit_path = main_dir.join(root.clone());
    let base_info_path = commit_path.join(BASE_FILE);
    let base_info = base_from_path(base_info_path.clone())?;
    for contract_hint in base_info.contract_hints {
        let contract_hex = hex::encode(contract_hint);
        let src_path = main_dir
            .join(MEMORY_DIR)
            .join(contract_hex.clone())
            .join(root.clone());
        let dst_path = main_dir.clone().join(MEMORY_DIR).join(contract_hex);
        for entry in fs::read_dir(src_path.clone())? {
            let entry = entry?;
            let filename = entry.file_name().to_string_lossy().to_string();
            let src_file_path = src_path.join(filename.clone());
            let dst_file_path = dst_path.join(filename);
            if src_file_path.is_file() {
                fs::rename(src_file_path, dst_file_path)?;
            }
        }
        fs::remove_dir(src_path.clone())?;
    }
    let index_path = commit_path.join(INDEX_FILE);
    let dst_index_path = main_dir.join(INDEX_FILE);
    fs::rename(index_path.clone(), dst_index_path.clone())?;

    let merkle_path = commit_path.join(MERKLE_FILE);
    let dst_merkle_path = main_dir.join(MERKLE_FILE);
    fs::rename(merkle_path.clone(), dst_merkle_path.clone())?;

    fs::remove_file(base_info_path)?;
    fs::remove_dir(commit_path.clone())?;

    Ok(())
}
