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

    for entry in fs::read_dir(root_dir)? {
        let entry = entry?;
        println!("entry={:?}", entry.path());
        if entry.path().is_dir() {
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
    println!("00003");
    let dir = dir.as_ref();

    let index_path = dir.join(INDEX_FILE);
    let index = index_from_path(index_path)?;
    println!("00004");

    let bytecode_dir = dir.join(BYTECODE_DIR);
    let memory_dir = dir.join(MEMORY_DIR);

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

        let memory_dir = memory_dir.join(&contract_hex);

        for page_index in &contract_index.page_indices {
            let page_path = page_path(&memory_dir, *page_index);
            if !page_path.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Non-existing memory for contract: {contract_hex}"),
                ));
            }
        }
    }

    let _main_commmit = commit_from_main_dir(engine, dir)?;

    Ok(Commit {
        index,
        paths: Vec::new(),
    })
}

fn commit_from_main_dir<P: AsRef<Path>>(
    engine: &Engine,
    dir: P,
) -> io::Result<Commit> {
    let dir = dir.as_ref();

    let index_dir_path = dir;
    let commit_id = dir.file_name().expect("Filename should exist");
    let main_path = dir
        .parent()
        .expect("Parent should exist")
        .parent()
        .expect("Parent should exist")
        .join(MAIN_DIR);
    println!(
        "COMMIT FROM MAIN DIR commit_id={:?} main_path={:?}",
        commit_id, main_path
    );
    let dir = main_path;

    let index_path = if index_dir_path.is_dir() {
        index_dir_path.join(INDEX_FILE)
    } else {
        dir.join(INDEX_FILE)
    };
    let index = index_from_path(index_path)?;

    let bytecode_dir = dir.join(BYTECODE_DIR);
    let _memory_dir = dir.join(MEMORY_DIR);

    for (contract, _contract_index) in index.iter() {
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

        // let memory_dir = memory_dir.join(&contract_hex);

        // for page_index in &contract_index.page_indices {
        //     let _page_path = page_path(&memory_dir, *page_index);
        // }
    }

    Ok(Commit {
        index,
        paths: Vec::new(),
    })
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
    paths: Vec<PathBuf>,
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
                println!("WRITE COMMIT START");
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
                println!("COMMIT DELETE {}", hex::encode(root));
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
                println!("COMMIT FINALIZE {}", hex::encode(root));
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
                println!("COMMIT HOLD {}", hex::encode(base));
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
                    println!("SESSION DROP {}", hex::encode(base));
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
    let commit_dir = root_dir.join(root_hex.clone());

    // Don't write the commit if it already exists on disk. This may happen if
    // the same transactions on the same base commit for example.
    if let Some(commit) = commits.get(&root) {
        return Ok(commit.clone());
    }

    match write_commit_inner(
        root_dir,
        &commit_dir,
        base,
        index,
        commit_contracts,
        root_hex,
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
fn write_commit_inner<P: AsRef<Path>, S: AsRef<str>>(
    root_dir: P,
    commit_dir: P,
    base: Option<Commit>,
    index: ContractIndex,
    commit_contracts: BTreeMap<ContractId, ContractDataEntry>,
    commit_id: S,
) -> io::Result<Commit> {
    println!(
        "WRITE_COMMIT_INNER: root_dir={:?} commit_dir={:?}",
        root_dir.as_ref(),
        commit_dir.as_ref()
    );
    let root_dir = root_dir.as_ref();
    let commit_dir = commit_dir.as_ref();
    let mut paths = Vec::new();

    struct Base {
        bytecode_dir: PathBuf,
        memory_dir: PathBuf,
        inner: Commit,
    }

    struct Directories {
        bytecode_dir: PathBuf,
        memory_dir: PathBuf,
        base: Option<Base>,
        main_dir: PathBuf,
        bytecode_main_dir: PathBuf,
        memory_main_dir: PathBuf,
    }

    let directories = {
        let bytecode_dir = commit_dir.join(BYTECODE_DIR);
        fs::create_dir_all(&bytecode_dir)?;

        let memory_dir = commit_dir.join(MEMORY_DIR);
        fs::create_dir_all(&memory_dir)?;

        let main_dir = root_dir
            .parent()
            .expect("root parent should exist")
            .join(MAIN_DIR);
        fs::create_dir_all(&main_dir)?;

        let bytecode_main_dir = main_dir.join(BYTECODE_DIR);
        fs::create_dir_all(&bytecode_main_dir)?;

        let memory_main_dir = main_dir.join(MEMORY_DIR);
        fs::create_dir_all(&memory_main_dir)?;

        Directories {
            bytecode_dir,
            memory_dir,
            base: base.map(|inner| {
                let base_root = *inner.index.root();

                let base_hex = hex::encode(base_root);
                let base_dir = root_dir.join(base_hex);

                Base {
                    bytecode_dir: base_dir.join(BYTECODE_DIR),
                    memory_dir: base_dir.join(MEMORY_DIR),
                    inner,
                }
            }),
            main_dir,
            bytecode_main_dir,
            memory_main_dir,
        }
    };

    // Write the dirty pages contracts of contracts to disk. If the contract
    // already existed in the base commit, we hard link
    for (contract, contract_data) in &commit_contracts {
        let contract_hex = hex::encode(contract);

        let memory_dir = directories.memory_dir.join(&contract_hex);

        let memory_main_dir = directories.memory_main_dir.join(&contract_hex);
        println!("MAIN_DIR={:?}", directories.main_dir);
        println!("MAIN_MEMORY_DIR={:?}", memory_main_dir);

        fs::create_dir_all(&memory_dir)?;
        fs::create_dir_all(&memory_main_dir)?;

        let mut pages = BTreeSet::new();

        // Write dirty pages and keep track of the page indices.
        let mut dp_count = 0;
        for (dirty_page, _, page_index) in contract_data.memory.dirty_pages() {
            let page_path1 = page_path(&memory_dir, *page_index);
            let page_path2: PathBuf = page_path_main(
                &memory_main_dir,
                *page_index,
                commit_id.as_ref(),
            )?;
            fs::write(page_path1.clone(), dirty_page)?;
            fs::write(page_path2.clone(), dirty_page)?;
            paths.push(page_path2.clone());
            println!("FILE WRITTEN {:?}", page_path1);
            println!("FILE WRITTEN MAIN {:?}", page_path2);
            pages.insert(*page_index);
            dp_count += 1;
        }
        println!("MEMORY_DIR {:?} dirty pages={}", memory_main_dir, dp_count);

        let bytecode_path = directories.bytecode_dir.join(&contract_hex);
        let module_path = bytecode_path.with_extension(OBJECTCODE_EXTENSION);
        let metadata_path = bytecode_path.with_extension(METADATA_EXTENSION);

        let bytecode_main_path =
            directories.bytecode_main_dir.join(&contract_hex);
        let module_main_path =
            bytecode_main_path.with_extension(OBJECTCODE_EXTENSION);
        let metadata_main_path =
            bytecode_main_path.with_extension(METADATA_EXTENSION);

        // If the contract is new, we write the bytecode, module, and metadata
        // files to disk, otherwise we hard link them to avoid duplicating them.
        //
        // Also, if there is a base commit, we hard link the pages of the
        // contracts that are not dirty.
        if contract_data.is_new {
            fs::write(bytecode_path, &contract_data.bytecode)?;
            fs::write(module_path, &contract_data.module.serialize())?;
            fs::write(metadata_path, &contract_data.metadata)?;
            // we write them to the main location
            fs::write(bytecode_main_path, &contract_data.bytecode)?;
            fs::write(module_main_path, &contract_data.module.serialize())?;
            fs::write(metadata_main_path, &contract_data.metadata)?;
        } else if let Some(base) = &directories.base {
            if let Some(elem) = base.inner.index.get(contract) {
                let base_bytecode_path = base.bytecode_dir.join(&contract_hex);
                let base_module_path =
                    base_bytecode_path.with_extension(OBJECTCODE_EXTENSION);
                let base_metadata_path =
                    base_bytecode_path.with_extension(METADATA_EXTENSION);

                let base_memory_dir = base.memory_dir.join(&contract_hex);

                fs::hard_link(base_bytecode_path, bytecode_path)?;
                fs::hard_link(base_module_path, module_path)?;
                fs::hard_link(base_metadata_path, metadata_path)?;

                for page_index in &elem.page_indices {
                    // Only write the clean pages, since the dirty ones have
                    // already been written.
                    if !pages.contains(page_index) {
                        let new_page_path = page_path(&memory_dir, *page_index);
                        let base_page_path =
                            page_path(&base_memory_dir, *page_index);

                        fs::hard_link(base_page_path, new_page_path)?;
                    }
                }
            }
        }
    }

    if let Some(base) = &directories.base {
        let mut bytecode_etc_hard_links = 0;
        let mut pages_hard_links = 0;
        for (contract, elem) in base.inner.index.iter() {
            if !commit_contracts.contains_key(contract) {
                let contract_hex = hex::encode(contract);

                let bytecode_path =
                    directories.bytecode_dir.join(&contract_hex);
                let module_path =
                    bytecode_path.with_extension(OBJECTCODE_EXTENSION);
                let metadata_path =
                    bytecode_path.with_extension(METADATA_EXTENSION);

                let base_bytecode_path = base.bytecode_dir.join(&contract_hex);
                let base_module_path =
                    base_bytecode_path.with_extension(OBJECTCODE_EXTENSION);
                let base_metadata_path =
                    base_bytecode_path.with_extension(METADATA_EXTENSION);

                let memory_dir = directories.memory_dir.join(&contract_hex);
                let base_memory_dir = base.memory_dir.join(&contract_hex);

                fs::create_dir_all(&memory_dir)?;

                // println!("HARD LINK BYTECODE {:?} {:?}", base_bytecode_path,
                // bytecode_path);
                fs::hard_link(base_bytecode_path, bytecode_path)?;
                // println!("HARD LINK MODULE {:?}", base_module_path);
                fs::hard_link(base_module_path, module_path)?;
                // println!("HARD LINK METADATA {:?}", base_metadata_path);
                fs::hard_link(base_metadata_path, metadata_path)?;
                bytecode_etc_hard_links += 1;

                for page_index in &elem.page_indices {
                    let new_page_path = page_path(&memory_dir, *page_index);
                    let base_page_path =
                        page_path(&base_memory_dir, *page_index);

                    fs::hard_link(base_page_path, new_page_path)?;
                    pages_hard_links += 1;
                }
            }
        }
        println!(
            "HARD LINK SUMMARY: bytecode_etc_hard_links={} pages_hard_links={}",
            bytecode_etc_hard_links, pages_hard_links
        );
    }

    let index_path = commit_dir.join(INDEX_FILE);
    let index_main_path = index_path_main(directories.main_dir, commit_id)?;
    let index_bytes = rkyv::to_bytes::<_, 128>(&index)
        .map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Failed serializing index file: {err}"),
            )
        })?
        .to_vec();
    fs::write(index_path, index_bytes.clone())?;
    println!("INDEX MAIN PATH={:?}", index_main_path);
    fs::write(index_main_path.clone(), index_bytes)?;
    paths.push(index_main_path);

    Ok(Commit { index, paths })
}

/// Delete the given commit's directory.
fn delete_commit_dir<P: AsRef<Path>>(
    root_dir: P,
    root: Hash,
) -> io::Result<()> {
    let root = hex::encode(root);
    println!("ACTUAL DELETION OF {}", root);
    let commit_dir = root_dir.as_ref().join(root);
    fs::remove_dir_all(commit_dir)
}

/// Finalize commit
fn finalize_commit<P: AsRef<Path>>(
    root: Hash,
    _root_dir: P,
    commit: &Commit,
) -> io::Result<()> {
    let root = hex::encode(root);
    println!("FINALIZATION OF {}", root);
    // let root_main_dir = root_dir
    //     .as_ref()
    //     .parent()
    //     .expect("Parent should exist")
    //     .join(MEMORY_DIR); // todo
    for src_path in commit.paths.iter() {
        let filename = src_path.file_name().expect("Filename should exist");
        let dst_dir = src_path
            .parent()
            .expect("Parent should exist")
            .parent()
            .expect("Parent should exist");
        let dst_path = dst_dir.join(filename);
        println!("finalize from={:?} to={:?}", src_path, dst_path);
        if src_path.is_file() {
            fs::rename(src_path, dst_path)?;
        }
        let src_dir = src_path.parent().expect("Parent should exist");
        let src_dir_empty = src_dir.read_dir()?.next().is_none(); // todo: make sure this performs well
        if src_dir_empty {
            let _ = fs::remove_dir(src_dir);
            println!("removed {:?}", src_dir);
        }
    }
    Ok(())
}
