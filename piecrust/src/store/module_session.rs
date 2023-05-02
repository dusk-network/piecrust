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

use crate::module::ModuleMetadata;
use piecrust_uplink::ModuleId;

use crate::store::tree::{position_from_module, Hash};
use crate::store::{
    compute_tree, Bytecode, Call, Commit, Memory, Metadata, Objectcode,
    BYTECODE_DIR, DIFF_EXTENSION, MEMORY_DIR, METADATA_EXTENSION,
    OBJECTCODE_EXTENSION,
};

#[derive(Debug, Clone)]
pub struct ModuleDataEntry {
    pub bytecode: Bytecode,
    pub objectcode: Objectcode,
    pub metadata: Metadata,
    pub memory: Memory,
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
#[derive(Debug)]
pub struct ModuleSession {
    modules: BTreeMap<ModuleId, ModuleDataEntry>,

    base: Option<Commit>,
    root_dir: PathBuf,

    call: mpsc::Sender<Call>,
}

impl ModuleSession {
    pub(crate) fn new<P: AsRef<Path>>(
        root_dir: P,
        base: Option<Commit>,
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
    pub fn root(&self) -> Hash {
        let (_, tree) = compute_tree(&self.base, &self.modules);
        *tree.root()
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
    pub fn commit(self) -> io::Result<Hash> {
        let mut slef = self;

        let (replier, receiver) = mpsc::sync_channel(1);

        let mut modules = BTreeMap::new();
        let mut base = slef.base.as_ref().map(|c| Commit {
            modules: BTreeMap::new(),
            diffs: BTreeSet::new(),
            tree: c.tree.clone(),
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
            .map(|c| *c.tree.root())
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
    ) -> io::Result<Option<ModuleDataEntry>> {
        match self.modules.entry(module) {
            Vacant(entry) => match &self.base {
                None => Ok(None),
                Some(base_commit) => {
                    let base = base_commit.tree.root();

                    match base_commit.modules.contains_key(&module) {
                        true => {
                            let base_hex = hex::encode(base);
                            let base_dir = self.root_dir.join(base_hex);

                            let module_hex = hex::encode(module);

                            let bytecode_path =
                                base_dir.join(BYTECODE_DIR).join(&module_hex);
                            let objectcode_path = bytecode_path
                                .with_extension(OBJECTCODE_EXTENSION);
                            let metadata_path = bytecode_path
                                .with_extension(METADATA_EXTENSION);
                            let memory_path =
                                base_dir.join(MEMORY_DIR).join(module_hex);
                            let memory_diff_path =
                                memory_path.with_extension(DIFF_EXTENSION);

                            let bytecode = Bytecode::from_file(bytecode_path)?;
                            let objectcode =
                                Objectcode::from_file(objectcode_path)?;
                            let metadata = Metadata::from_file(metadata_path)?;
                            let memory =
                                match base_commit.diffs.contains(&module) {
                                    true => Memory::from_file_and_diff(
                                        memory_path,
                                        memory_diff_path,
                                    )?,
                                    false => Memory::from_file(memory_path)?,
                                };

                            let module = entry
                                .insert(ModuleDataEntry {
                                    bytecode,
                                    objectcode,
                                    metadata,
                                    memory,
                                })
                                .clone();

                            Ok(Some(module))
                        }
                        false => Ok(None),
                    }
                }
            },
            Occupied(entry) => Ok(Some(entry.get().clone())),
        }
    }

    /// Clear all deployed deployed or otherwise instantiated modules.
    pub fn clear_modules(&mut self) {
        self.modules.clear();
    }

    /// Checks if module is deployed
    pub fn module_deployed(&mut self, module_id: ModuleId) -> bool {
        if self.modules.contains_key(&module_id) {
            true
        } else if let Some(base_commit) = &self.base {
            base_commit.modules.contains_key(&module_id)
        } else {
            false
        }
    }

    /// Deploys bytecode to the module store with the given its `module_id`.
    ///
    /// See [`deploy`] for deploying bytecode without specifying a module ID.
    ///
    /// [`deploy`]: ModuleSession::deploy
    pub fn deploy<B: AsRef<[u8]>>(
        &mut self,
        module_id: ModuleId,
        bytecode: B,
        objectcode: B,
        metadata: ModuleMetadata,
        metadata_bytes: B,
    ) -> io::Result<()> {
        let memory = Memory::new()?;
        let bytecode = Bytecode::new(bytecode)?;
        let objectcode = Objectcode::new(objectcode)?;
        let metadata = Metadata::new(metadata_bytes, metadata)?;

        // If the position is already filled in the tree, the module cannot be
        // inserted.
        if let Some(base) = self.base.as_ref() {
            let pos = position_from_module(&module_id);
            if base.tree.contains(pos) {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("Existing module at position '{pos}' in tree"),
                ));
            }
        }

        self.modules.insert(
            module_id,
            ModuleDataEntry {
                bytecode,
                objectcode,
                metadata,
                memory,
            },
        );

        Ok(())
    }

    /// Provides metadata of the module with a given `module_id`.
    pub fn module_metadata(
        &self,
        module_id: &ModuleId,
    ) -> Option<&ModuleMetadata> {
        self.modules
            .get(module_id)
            .map(|store_data| store_data.metadata.data())
    }
}

impl Drop for ModuleSession {
    fn drop(&mut self) {
        if let Some(base) = self.base.take() {
            let root = base.tree.root();
            let _ = self.call.send(Call::SessionDrop(*root));
        }
    }
}
