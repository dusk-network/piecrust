// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::collections::btree_map::Entry::{Occupied, Vacant};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::{io, mem};

use piecrust_uplink::ModuleId;

use crate::store::{
    compute_root, Bytecode, Call, Commit, Memory, Root, BYTECODE_DIR,
    DIFF_EXTENSION, MEMORY_DIR,
};

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
    pub(crate) fn new<P: AsRef<Path>>(
        root_dir: P,
        base: Option<(Root, Commit)>,
        call: mpsc::Sender<Call>,
    ) -> Self {
        Self {
            modules: BTreeMap::new(),
            base,
            root_dir: root_dir.as_ref().into(),
            call,
        }
    }

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
        let (root, _) = compute_root(&self.base, &self.modules);
        root
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
                    modules: BTreeMap::new(),
                    diffs: BTreeSet::new(),
                },
            )
        });

        mem::swap(&mut slef.modules, &mut modules);
        mem::swap(&mut slef.base, &mut base);

        slef.call
            .send(Call::Commit {
                modules,
                base,
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
                    match base_commit.modules.contains_key(&module) {
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

        let module_id = ModuleId::from_bytes(hash.into());
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
    ) -> io::Result<()> {
        if self.modules.contains_key(&module_id) {
            let module_hex = hex::encode(module_id);
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Failed deploying {module_hex}: already deployed"),
            ));
        }

        if let Some((base, base_commit)) = &self.base {
            if base_commit.modules.contains_key(&module_id) {
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

        Ok(())
    }
}

impl Drop for ModuleSession {
    fn drop(&mut self) {
        if let Some((base, _)) = self.base.take() {
            let _ = self.call.send(Call::SessionDrop(base));
        }
    }
}
