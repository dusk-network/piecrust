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

use std::collections::btree_map::Entry::*;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Debug, Formatter};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::SystemTime;
use std::{fs, io, thread};

use dusk_wasmtime::Engine;
use piecrust_uplink::ContractId;
use session::ContractDataEntry;
use tree::{ContractIndex, Hash};

pub use bytecode::Bytecode;
pub use memory::{Memory, PAGE_SIZE};
pub use metadata::Metadata;
pub use module::Module;
pub use session::ContractSession;
pub use tree::PageOpening;

const BYTECODE_DIR: &str = "bytecode";
const MEMORY_DIR: &str = "memory";
const INDEX_FILE: &str = "index";
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
        let root = *commit.index.root();
        commits.insert(root, commit);
    }

    for entry in fs::read_dir(root_dir)? {
        let entry = entry?;
        println!("entry={:?}", entry.path());
        if entry.path().is_dir() {
            let filename = entry.file_name().to_string_lossy().to_string();
            if filename == MEMORY_DIR || filename == BYTECODE_DIR {
                continue;
            }
            let commit = read_commit(engine, entry.path())?;
            let root = *commit.index.root();
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
        // todo: this means we are in a commit dir, need to back up for bytecode
        // and memory paths to work correctly
        dir.parent().expect("Parent should exist")
    } else {
        dir
    };

    let index_path = dir.join(INDEX_FILE);
    println!("index_path={:?}", index_path);
    let index = index_from_path(index_path)?;

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
                let maybe_hash = commit_id.as_ref().map(|s| {
                    let hash: [u8; 32] = hex::decode(s)
                        .expect("Hex decoding of commit id should succeed")
                        .try_into()
                        .unwrap();
                    Hash::from(hash)
                });
                let path = ContractSession::do_find_page(
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

    Ok(Commit { index })
}

fn index_from_path<P: AsRef<Path>>(path: P) -> io::Result<ContractIndex> {
    let path = path.as_ref();

    let index_bytes = fs::read(path)?;
    let index = rkyv::from_bytes(&index_bytes).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Invalid index file \"{path:?}\": {err}"),
        )
    })?;

    Ok(index)
}

#[derive(Debug, Default, Clone)]
pub(crate) struct Commit {
    index: ContractIndex,
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
            // Writes a session to disk and adds it to the map of existing commits.
            Call::Commit {
                contracts,
                base,
                replier,
            } => {
                let start = SystemTime::now();
                println!("**WRITE COMMIT START");
                let io_result = write_commit(root_dir, &mut commits, base, contracts);
                let stop = SystemTime::now();
                println!(
                    "WRITE COMMIT FINISHED, ELAPSED TIME={:?}",
                    stop.duration_since(start).expect("duration should work")
                );
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
                println!("**COMMIT DELETE {}", hex::encode(root));
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
            // Finalize commit
            Call::CommitFinalize { commit: root, replier } => {
                println!("**COMMIT FINALIZE {}", hex::encode(root));
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
                    let io_result = finalize_commit(root, root_dir, &commit);
                    commits.remove(&root);
                    let _ = replier.send(io_result);
                } else {
                    let _ = replier.send(Ok(())); // todo: find better way
                }
            }
            // Increment the hold count of a commit to prevent it from deletion
            // on a `Call::CommitDelete`.
            Call::CommitHold {
                base,
                replier,
            } => {
                println!("**COMMIT HOLD {}", hex::encode(base));
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
                    println!("**SESSION DROP {}", hex::encode(base));
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
    println!("ROOT DIR = {:?}", root_dir.as_ref());
    let root_dir = root_dir.as_ref();

    let mut index = base
        .as_ref()
        .map_or(ContractIndex::default(), |base| base.index.clone());

    for (contract_id, contract_data) in &commit_contracts {
        if contract_data.is_new {
            index.remove_and_insert(*contract_id, &contract_data.memory);
        } else {
            index.insert(*contract_id, &contract_data.memory);
        }
    }

    let root = *index.root();
    let root_hex = hex::encode(root);
    println!("COMMIT ID = {}", root_hex);

    // Don't write the commit if it already exists on disk. This may happen if
    // the same transactions on the same base commit for example.
    if let Some(commit) = commits.get(&root) {
        return Ok(commit.clone());
    }

    match write_commit_inner(root_dir, index, commit_contracts, root_hex, base)
    {
        Ok(commit) => {
            commits.insert(root, commit.clone());
            Ok(commit)
        }
        Err(err) => Err(err),
    }
}

/// Writes a commit to disk.
fn write_commit_inner<P: AsRef<Path>, S: AsRef<str>>(
    root_dir: P,
    mut index: ContractIndex,
    commit_contracts: BTreeMap<ContractId, ContractDataEntry>,
    commit_id: S,
    maybe_base: Option<Commit>,
) -> io::Result<Commit> {
    println!(
        "WRITE_COMMIT_INNER: root_dir={:?} commit contracts={:?}",
        root_dir.as_ref(),
        commit_contracts.keys()
    );
    let root_dir = root_dir.as_ref();
    index.contract_hints.clear();
    index.maybe_base = maybe_base.map(|base| *base.index.root());

    struct Directories {
        main_dir: PathBuf,
        bytecode_main_dir: PathBuf,
        memory_main_dir: PathBuf,
    }

    let directories = {
        let main_dir = root_dir.join(MAIN_DIR);
        fs::create_dir_all(&main_dir)?;
        println!("created1 {:?}", main_dir);

        let bytecode_main_dir = main_dir.join(BYTECODE_DIR);
        fs::create_dir_all(&bytecode_main_dir)?;
        println!("created2 {:?}", bytecode_main_dir);

        let memory_main_dir = main_dir.join(MEMORY_DIR);
        fs::create_dir_all(&memory_main_dir)?;
        println!("created3 {:?}", memory_main_dir);

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
            index.contract_hints.push(contract.clone());
        }
    }

    let index_main_path = index_path_main(directories.main_dir, commit_id)?;
    let index_bytes = rkyv::to_bytes::<_, 128>(&index)
        .map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Failed serializing index file: {err}"),
            )
        })?
        .to_vec();
    fs::write(index_main_path.clone(), index_bytes)?;

    Ok(Commit { index })
}

/// Delete the given commit's directory.
fn delete_commit_dir<P: AsRef<Path>>(
    root_dir: P,
    root: Hash,
) -> io::Result<()> {
    let root = hex::encode(root);
    println!("ACTUAL DELETION OF {}", root);
    let root_main_dir = root_dir.as_ref().join(MAIN_DIR);
    let commit_dir = root_main_dir.join(root.clone());
    if commit_dir.exists() {
        let index_path = commit_dir.join(INDEX_FILE);
        let index = index_from_path(index_path.clone())?;
        for contract_hint in index.contract_hints {
            let contract_hex = hex::encode(contract_hint);
            let commit_mem_path = root_main_dir
                .join(MEMORY_DIR)
                .join(contract_hex.clone())
                .join(root.clone());
            fs::remove_dir_all(commit_mem_path.clone())?;
            println!("DELETE {:?}", commit_mem_path)
        }
        fs::remove_dir_all(commit_dir.clone())?;
        println!("DELETE {:?}", commit_dir)
    } else {
        println!("DELETE did not exist: {:?} ", commit_dir)
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
    println!("FINALIZATION OF {} in main dir={:?}", root, main_dir);
    let commit_path = main_dir.join(root.clone());
    let index_path = commit_path.join(INDEX_FILE);
    let index = index_from_path(index_path.clone())?;
    // println!("index_path = {:?}", index_path);
    for contract_hint in index.contract_hints {
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
            // println!(
            //     "finalize2 from={:?} to={:?}",
            //     src_file_path, dst_file_path
            // );
            if src_file_path.is_file() {
                fs::rename(src_file_path, dst_file_path)?;
            }
        }
        fs::remove_dir(src_path.clone())?;
        // println!("finalize2 from={:?} to={:?}", src_path, dst_path);
        // println!("removed2 {:?}", src_path);
    }
    let dst_index_path = main_dir.join(INDEX_FILE);
    fs::rename(index_path.clone(), dst_index_path.clone())?;

    // load index
    let mut main_index = index_from_path(dst_index_path.clone())?;
    // clear contract hints
    main_index.contract_hints.clear();
    // clear base
    main_index.maybe_base = None;
    // save index
    let index_bytes = rkyv::to_bytes::<_, 128>(&main_index)
        .map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Failed serializing index file: {err}"),
            )
        })?
        .to_vec();
    fs::write(dst_index_path.clone(), index_bytes)?;

    // println!(
    //     "finalize2 index from={:?} to={:?}",
    //     index_path, dst_index_path
    // );
    fs::remove_dir(commit_path.clone())?;
    // println!("finalize2 removed {:?}", commit_path);

    // todo: this is a temporary diagnostic code
    for entry in main_dir.read_dir()? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().starts_with("fin_") {
            fs::remove_file(entry.path())?;
        }
    }
    // todo: this is a temporary diagnostic code
    fs::write(main_dir.join(format!("fin_{}", root)), "f")?;
    Ok(())
}
