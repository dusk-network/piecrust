//! A library for dealing with memories in trees.

mod bytecode;
mod memory;

use std::collections::btree_map::Entry::*;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::{fs, io, mem, thread};

pub use bytecode::Bytecode;
pub use memory::Memory;

use flate2::write::DeflateEncoder;
use flate2::Compression;

const ROOT_LEN: usize = 32;
const MODULE_ID_LEN: usize = 32;

const BYTECODE_DIR: &str = "bytecode";
const MEMORY_DIR: &str = "memory";
const DIFF_EXTENSION: &str = "diff";

type Root = [u8; ROOT_LEN];
type ModuleId = [u8; MODULE_ID_LEN];

/// A store for all module commits.
pub struct ModuleStore {
    sync_loop: thread::JoinHandle<()>,
    call: mpsc::Sender<Call>,
    root_dir: PathBuf,
}

impl ModuleStore {
    /// Loads a new module store from the given `dir`ectory.
    ///
    /// This also starts the synchronization loop, which is used to align
    /// [`commit`]s, [`delete`]s, and [`session spawning`] to avoid deleting
    /// commits in use by a session.
    ///
    /// [`commit`]: ModuleSession::commit
    /// [`delete`]: ModuleStore::delete_commit
    /// [`session spawning`]: ModuleStore::session
    pub fn new<P: AsRef<Path>>(dir: P) -> io::Result<Self> {
        let root_dir = dir.as_ref();

        fs::create_dir_all(root_dir)?;

        let (call, calls) = mpsc::channel();
        let commits = read_all_commits(root_dir)?;

        let loop_root_dir = root_dir.to_path_buf();
        let sync_loop =
            thread::spawn(|| sync_loop(loop_root_dir, commits, calls));

        Ok(Self {
            sync_loop,
            call,
            root_dir: root_dir.into(),
        })
    }

    /// Create a new [`ModuleSession`] with the given `base` commit.
    ///
    /// Errors if the given base commit does not exist in the store.
    pub fn session(&self, base: Root) -> io::Result<ModuleSession> {
        let (replier, receiver) = mpsc::sync_channel(1);

        self.call.send(Call::SessionHold { base, replier }).expect(
            "The receiver should never be dropped while there are senders",
        );

        let base_commit = receiver
            .recv()
            .expect("The sender in the loop should never be dropped without responding", )
            .ok_or(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("No such base commit: {}", hex::encode(base)),
            ))?;

        Ok(self.session_with_base(Some((base, base_commit))))
    }

    /// Create a new [`ModuleSession`] that has no base commit.
    ///
    /// For session with a base commit, please see [`session`].
    ///
    /// [`session`]: ModuleStore::session
    pub fn genesis_session(&self) -> ModuleSession {
        self.session_with_base(None)
    }

    /// Deletes a given `commit` from the store.
    ///
    /// If a `ModuleSession` is currently using the given commit as a base, the
    /// operation will be queued for completion until the last session using the
    /// commit has dropped.
    ///
    /// It will block until the operation is completed.
    pub fn delete_commit(&self, commit: Root) -> io::Result<()> {
        let (replier, receiver) = mpsc::sync_channel(1);

        self.call
            .send(Call::CommitDelete { commit, replier })
            .expect(
                "The receiver should never be dropped while there are senders",
            );

        receiver.recv().expect(
            "The sender in the loop should never be dropped without responding",
        )
    }

    /// Return the handle to the thread running the store's synchronization
    /// loop.
    pub fn sync_loop(&self) -> &thread::Thread {
        self.sync_loop.thread()
    }

    fn session_with_base(&self, base: Option<(Root, Commit)>) -> ModuleSession {
        ModuleSession {
            modules: BTreeMap::new(),
            base,
            root_dir: self.root_dir.clone(),
            call: self.call.clone(),
        }
    }
}

fn read_all_commits<P: AsRef<Path>>(
    root_dir: P,
) -> io::Result<BTreeMap<Root, Commit>> {
    let root_dir = root_dir.as_ref();
    let mut commits = BTreeMap::new();

    for entry in fs::read_dir(root_dir)? {
        let entry = entry?;
        let (root, commit) = read_commit(entry.path())?;
        commits.insert(root, commit);
    }

    Ok(commits)
}

fn read_commit<P: AsRef<Path>>(commit_dir: P) -> io::Result<(Root, Commit)> {
    let commit_dir = commit_dir.as_ref();

    let root = root_from_dir(commit_dir)?;
    let commit = commit_from_dir(commit_dir)?;

    Ok((root, commit))
}

fn root_from_dir<P: AsRef<Path>>(dir: P) -> io::Result<Root> {
    let dir = dir.as_ref();
    let dir_name = dir.file_name().unwrap().to_str().ok_or(io::Error::new(
        io::ErrorKind::InvalidData,
        format!("Directory name \"{dir:?}\" is invalid UTF-8"),
    ))?;

    let dir_name_bytes = hex::decode(dir_name).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Directory name \"{dir_name}\" is invalid hex: {err}"),
        )
    })?;

    if dir_name_bytes.len() != ROOT_LEN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Expected directory name \"{dir_name}\" to be of size {ROOT_LEN}, was {}", dir_name_bytes.len())
        ));
    }

    let mut root = [0u8; ROOT_LEN];
    root.copy_from_slice(&dir_name_bytes);

    Ok(root)
}

fn commit_from_dir<P: AsRef<Path>>(dir: P) -> io::Result<Commit> {
    let dir = dir.as_ref();

    let mut bytecode = BTreeSet::new();

    let bytecode_dir = dir.join(BYTECODE_DIR);
    for bytecode_entry in fs::read_dir(bytecode_dir)? {
        let entry = bytecode_entry?;
        let module = module_id_from_path(entry.path())?;
        bytecode.insert(module);
    }

    let mut memory = BTreeSet::new();
    let mut diffs = BTreeSet::new();

    let memory_dir = dir.join(MEMORY_DIR);
    for memory_entry in fs::read_dir(memory_dir)? {
        let entry = memory_entry?;
        let mut entry_path = entry.path();

        // If the file has an extension and it is DIFF_EXTENSION, it's a diff.
        // If the file has an extension, but it is *not* DIFF_EXTENSION just
        // ignore the file.
        if let Some(extension) = entry_path.extension() {
            if extension == DIFF_EXTENSION {
                entry_path.set_extension("");
                let module = module_id_from_path(entry_path)?;
                diffs.insert(module);
            }
            continue;
        }

        let module = module_id_from_path(entry.path())?;
        memory.insert(module);
    }

    // Ensure all diffs are of modules that exist
    for module in &diffs {
        if !memory.contains(module) {
            let module_hex = hex::encode(module);
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Non-existing module for diff {module_hex}"),
            ));
        }
    }

    if bytecode != memory {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Inconsistent commit directory: {dir:?}"),
        ));
    }

    let modules = bytecode;
    Ok(Commit { modules, diffs })
}

fn module_id_from_path<P: AsRef<Path>>(path: P) -> io::Result<ModuleId> {
    let path = path.as_ref();
    let path_name =
        path.file_name().unwrap().to_str().ok_or(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("File name \"{path:?}\" is invalid UTF-8"),
        ))?;

    let path_name_bytes = hex::decode(path_name).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("File name \"{path_name}\" is invalid hex: {err}"),
        )
    })?;

    if path_name_bytes.len() != ROOT_LEN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Expected file name \"{path_name}\" to be of size {MODULE_ID_LEN}, was {}", path_name_bytes.len())
        ));
    }

    let mut module = [0u8; MODULE_ID_LEN];
    module.copy_from_slice(&path_name_bytes);

    Ok(module)
}

#[derive(Clone)]
struct Commit {
    modules: BTreeSet<ModuleId>,
    diffs: BTreeSet<ModuleId>,
}

enum Call {
    Commit {
        modules: BTreeMap<ModuleId, (Bytecode, Memory)>,
        base: Option<Root>,
        replier: mpsc::SyncSender<io::Result<(Root, Commit)>>,
    },
    CommitDelete {
        commit: Root,
        replier: mpsc::SyncSender<io::Result<()>>,
    },
    SessionHold {
        base: Root,
        replier: mpsc::SyncSender<Option<Commit>>,
    },
    SessionDrop(Root),
}

fn sync_loop<P: AsRef<Path>>(
    root_dir: P,
    commits: BTreeMap<Root, Commit>,
    calls: mpsc::Receiver<Call>,
) {
    let root_dir = root_dir.as_ref();

    let mut sessions = BTreeMap::new();
    let mut delete_bag = BTreeMap::new();
    let mut commits = commits;

    for call in calls {
        match call {
            // Writes a session to disk and adds it to the map of existing commits.
            Call::Commit {
                modules,
                base,
                replier,
            } => {
                let io_result = write_commit(root_dir, &mut commits, base, modules);
                let _ = replier.send(io_result);
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
                let _ = replier.send(io_result);
            }
            // Increment the hold count of a commit to prevent it from deletion
            // on a `Call::CommitDelete`.
            Call::SessionHold {
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

                        match delete_bag.entry(base) {
                            Vacant(_) => {}
                            Occupied(entry) => {
                                for replier in entry.remove() {
                                    let io_result =
                                        delete_commit_dir(root_dir, base);
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
    commits: &mut BTreeMap<Root, Commit>,
    base: Option<Root>,
    modules: BTreeMap<ModuleId, (Bytecode, Memory)>,
) -> io::Result<(Root, Commit)> {
    let root_dir = root_dir.as_ref();

    let root = compute_root(&modules);
    let root_hex = hex::encode(root);

    let commit_dir = root_dir.join(root_hex);

    match write_commit_inner(root_dir, &commit_dir, commits, base, modules) {
        Ok(commit) => {
            commits.insert(root, commit.clone());
            Ok((root, commit))
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
    commits: &BTreeMap<Root, Commit>,
    base: Option<Root>,
    modules: BTreeMap<ModuleId, (Bytecode, Memory)>,
) -> io::Result<Commit> {
    let root_dir = root_dir.as_ref();
    let commit_dir = commit_dir.as_ref();

    let bytecode_dir = commit_dir.join(BYTECODE_DIR);
    fs::create_dir_all(&bytecode_dir)?;

    let memory_dir = commit_dir.join(MEMORY_DIR);
    fs::create_dir_all(&memory_dir)?;

    let mut commit_modules = BTreeSet::new();
    let mut commit_diff_modules = BTreeSet::new();

    match base {
        None => {
            for (module, (bytecode, memory)) in modules {
                let module_hex = hex::encode(module);

                let bytecode_path = bytecode_dir.join(&module_hex);
                let memory_path = memory_dir.join(&module_hex);

                fs::write(bytecode_path, &bytecode)?;
                fs::write(memory_path, memory.lock().as_ref())?;

                commit_modules.insert(module);
            }
        }
        Some(base) => {
            let base_commit =
                commits.get(&base).expect("Parent commit must be in map");

            let base_hex = hex::encode(base);
            let base_dir = root_dir.join(base_hex);

            let base_bytecode_dir = base_dir.join(BYTECODE_DIR);
            let base_memory_dir = base_dir.join(MEMORY_DIR);

            for module in &base_commit.modules {
                let module_hex = hex::encode(module);

                let bytecode_path = bytecode_dir.join(&module_hex);
                let memory_path = memory_dir.join(&module_hex);

                let base_bytecode_path = base_bytecode_dir.join(&module_hex);
                let base_memory_path = base_memory_dir.join(&module_hex);

                fs::hard_link(base_bytecode_path, bytecode_path)?;
                fs::hard_link(base_memory_path, memory_path)?;

                commit_modules.insert(*module);
            }

            for (module, (bytecode, memory)) in modules {
                let module_hex = hex::encode(module);

                match base_commit.modules.contains(&module) {
                    true => {
                        let base_memory_path =
                            base_memory_dir.join(&module_hex);
                        let memory_diff_path = memory_dir
                            .join(&module_hex)
                            .with_extension(DIFF_EXTENSION);

                        let base_memory = Memory::from_file(base_memory_path)?;
                        let memory_diff = File::create(memory_diff_path)?;

                        let mut encoder = DeflateEncoder::new(
                            memory_diff,
                            Compression::default(),
                        );

                        bsdiff::diff::diff(
                            base_memory.lock().as_ref(),
                            memory.lock().as_ref(),
                            &mut encoder,
                        )?;

                        commit_diff_modules.insert(module);
                    }
                    false => {
                        let bytecode_path = bytecode_dir.join(&module_hex);
                        let memory_path = memory_dir.join(&module_hex);

                        fs::write(bytecode_path, &bytecode)?;
                        fs::write(memory_path, memory.lock().as_ref())?;

                        commit_modules.insert(module);
                    }
                }
            }
        }
    }

    Ok(Commit {
        modules: commit_modules,
        diffs: commit_diff_modules,
    })
}

/// Arrange the given memories in a tree, and compute the root hash of that
/// tree.
fn compute_root<'a, I>(modules: I) -> Root
where
    I: IntoIterator<Item = (&'a ModuleId, &'a (Bytecode, Memory))>,
    I::IntoIter: ExactSizeIterator,
{
    let iter = modules.into_iter();
    let size = iter.len();

    let mut leaves = Vec::with_capacity(size);
    for (module, (bytecode, memory)) in iter {
        let mut hasher = blake3::Hasher::new();

        hasher.update(module);
        hasher.update(bytecode.as_ref());
        hasher.update(memory.lock().as_ref());

        leaves.push(Root::from(hasher.finalize()));
    }

    while leaves.len() > 1 {
        leaves = leaves
            .chunks(2)
            .map(|chunk| {
                let mut hasher = blake3::Hasher::new();

                hasher.update(&chunk[0]);
                if chunk.len() > 1 {
                    hasher.update(&chunk[1]);
                }

                Root::from(hasher.finalize())
            })
            .collect();
    }

    leaves[0]
}

/// Delete the given commit's directory.
fn delete_commit_dir<P: AsRef<Path>>(
    root_dir: P,
    root: Root,
) -> io::Result<()> {
    let root = hex::encode(root);
    let commit_dir = root_dir.as_ref().join(root);
    fs::remove_dir_all(commit_dir)
}

/// The representation of a session with a [`ModuleStore`].
///
/// A session tracks modifications to the modules' memories by keeping
/// references to the set of instantiated modules.
///
/// The modifications are kept in memory and are only persisted to disk on a
/// call to [`commit`].
///
/// [`commit`]: ModuleSession::commit
pub struct ModuleSession {
    modules: BTreeMap<ModuleId, (Bytecode, Memory)>,

    base: Option<(Root, Commit)>,
    root_dir: PathBuf,

    call: mpsc::Sender<Call>,
}

impl ModuleSession {
    /// Returns the root that the session would have if one would decide to
    /// commit it.
    ///
    /// Keep in mind that modifications to memories obtained using [`module`],
    /// may cause the root to be inconsistent. The caller should ensure that no
    /// instance of [`Memory`] obtained via this session is being modified when
    /// calling this function.
    ///
    /// [`module`]: ModuleSession::module
    pub fn root(&self) -> Root {
        compute_root(&self.modules)
    }

    /// Commits the given session to disk, consuming the session and adding it
    /// to the [`ModuleStore`] it was created from.
    ///
    /// Keep in mind that modifications to memories obtained using [`module`],
    /// may cause the root to be inconsistent. The caller should ensure that no
    /// instance of [`Memory`] obtained via this session is being modified when
    /// calling this function.
    ///
    /// [`module`]: ModuleSession::module
    pub fn commit(self) -> io::Result<Root> {
        let mut slef = self;

        let (replier, receiver) = mpsc::sync_channel(1);

        let mut modules = BTreeMap::new();
        let mut base = slef.base.as_ref().map(|(root, _)| {
            (
                *root,
                Commit {
                    modules: BTreeSet::new(),
                    diffs: BTreeSet::new(),
                },
            )
        });

        mem::swap(&mut slef.modules, &mut modules);
        mem::swap(&mut slef.base, &mut base);

        slef.call
            .send(Call::Commit {
                modules,
                base: base.map(|p| p.0),
                replier,
            })
            .expect("The receiver should never drop before sending");

        receiver
            .recv()
            .expect("The receiver should always receive a reply")
            .map(|p| p.0)
    }

    /// Return the bytecode and memory belonging to the given `module`, if it
    /// exists.
    ///
    /// The module is considered to exist if either of the following conditions
    /// are met:
    ///
    /// - The module has been [`deploy`]ed in this session
    /// - The module was deployed to the base commit
    ///
    /// [`deploy`]: ModuleSession::deploy
    pub fn module(
        &mut self,
        module: ModuleId,
    ) -> io::Result<Option<(Bytecode, Memory)>> {
        match self.modules.entry(module) {
            Vacant(entry) => match &self.base {
                None => Ok(None),
                Some((base, base_commit)) => {
                    match base_commit.modules.contains(&module) {
                        true => {
                            let base_hex = hex::encode(base);
                            let base_dir = self.root_dir.join(base_hex);

                            let module_hex = hex::encode(module);

                            let bytecode_path =
                                base_dir.join(BYTECODE_DIR).join(&module_hex);
                            let memory_path =
                                base_dir.join(MEMORY_DIR).join(module_hex);
                            let memory_diff_path =
                                memory_path.with_extension(DIFF_EXTENSION);

                            let bytecode = Bytecode::from_file(bytecode_path)?;
                            let memory =
                                match base_commit.diffs.contains(&module) {
                                    true => Memory::from_file_and_diff(
                                        memory_path,
                                        memory_diff_path,
                                    )?,
                                    false => Memory::from_file(memory_path)?,
                                };

                            let module =
                                entry.insert((bytecode, memory)).clone();

                            Ok(Some(module))
                        }
                        false => Ok(None),
                    }
                }
            },
            Occupied(entry) => Ok(Some(entry.get().clone())),
        }
    }

    /// Deploys bytecode to the module store.
    ///
    /// The module ID returned is computed using the `blake3` hash of the given
    /// bytecode. See [`deploy_with_id`] for deploying bytecode with a given
    /// module ID.
    ///
    /// [`deploy_with_id`]: ModuleSession::deploy_with_id
    pub fn deploy<B: AsRef<[u8]>>(
        &mut self,
        bytecode: B,
    ) -> io::Result<ModuleId> {
        let bytes = bytecode.as_ref();
        let hash = blake3::hash(bytes);

        let module_id = hash.into();
        self.deploy_with_id(module_id, bytes)?;

        Ok(module_id)
    }

    /// Deploys bytecode to the module store with the given its `module_id`.
    ///
    /// See [`deploy`] for deploying bytecode without specifying a module ID.
    ///
    /// [`deploy`]: ModuleSession::deploy
    pub fn deploy_with_id<B: AsRef<[u8]>>(
        &mut self,
        module_id: ModuleId,
        bytecode: B,
    ) -> io::Result<ModuleId> {
        if self.modules.contains_key(&module_id) {
            let module_hex = hex::encode(module_id);
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Failed deploying {module_hex}: already deployed"),
            ));
        }

        if let Some((base, base_commit)) = &self.base {
            if base_commit.modules.contains(&module_id) {
                let module_hex = hex::encode(module_id);
                let base_hex = hex::encode(base);
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("Failed deploying {module_hex}: already deployed in base commit {base_hex}"),
                ));
            }
        }

        let memory = Memory::new()?;
        let bytecode = Bytecode::new(bytecode)?;

        self.modules.insert(module_id, (bytecode, memory));

        Ok(module_id)
    }
}

impl Drop for ModuleSession {
    fn drop(&mut self) {
        if let Some((base, _)) = self.base.take() {
            let _ = self.call.send(Call::SessionDrop(base));
        }
    }
}
