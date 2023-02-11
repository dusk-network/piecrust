// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

mod commit_path;
mod diff_data;
mod module_commit;
mod module_commit_bag;

use bytecheck::CheckBytes;
use rkyv::{Archive, Deserialize, Serialize};

use piecrust_uplink::ModuleId;

use rand::Rng;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::error::Error::{self, PersistenceError, RestoreError};
use crate::merkle::Merkle;
use crate::persistable::Persistable;

use crate::memory_path::MemoryPath;
pub use commit_path::CommitPath;
pub use module_commit::{ModuleCommit, ModuleCommitLike};
pub use module_commit_bag::{BagSizeInfo, ModuleCommitBag};

pub const COMMIT_ID_BYTES: usize = 32;

pub trait Hashable {
    fn uninitialized() -> Self;
    fn as_slice(&self) -> &[u8];
}

#[derive(
    PartialEq,
    Eq,
    Archive,
    Serialize,
    CheckBytes,
    Deserialize,
    PartialOrd,
    Ord,
    Hash,
    Clone,
    Copy,
)]
#[archive(as = "Self")]
#[repr(C)]
pub struct ModuleCommitId([u8; COMMIT_ID_BYTES]);

impl ModuleCommitId {
    pub fn from_hash_of(mem: &[u8]) -> Result<Self, Error> {
        Ok(ModuleCommitId(*blake3::hash(mem).as_bytes()))
    }

    pub const fn from_bytes(bytes: [u8; COMMIT_ID_BYTES]) -> Self {
        Self(bytes)
    }

    pub fn random() -> Self {
        ModuleCommitId(rand::thread_rng().gen::<[u8; COMMIT_ID_BYTES]>())
    }

    pub const fn to_bytes(self) -> [u8; COMMIT_ID_BYTES] {
        self.0
    }
}

impl Hashable for ModuleCommitId {
    fn uninitialized() -> Self {
        ModuleCommitId([0; COMMIT_ID_BYTES])
    }

    fn as_slice(&self) -> &[u8] {
        &self.0[..]
    }
}

impl core::fmt::Debug for ModuleCommitId {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        if f.alternate() {
            write!(f, "0x")?
        }
        for byte in self.0 {
            write!(f, "{:02x}", &byte)?
        }
        Ok(())
    }
}

impl Persistable for ModuleCommitId {}

impl AsRef<[u8]> for ModuleCommitId {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<[u8; COMMIT_ID_BYTES]> for ModuleCommitId {
    fn from(bytes: [u8; COMMIT_ID_BYTES]) -> Self {
        Self::from_bytes(bytes)
    }
}

impl From<ModuleCommitId> for [u8; COMMIT_ID_BYTES] {
    fn from(commit: ModuleCommitId) -> Self {
        commit.to_bytes()
    }
}

#[derive(
    PartialEq,
    Eq,
    Archive,
    Serialize,
    CheckBytes,
    Deserialize,
    PartialOrd,
    Ord,
    Hash,
    Clone,
    Copy,
)]
#[archive(as = "Self")]
#[repr(C)]
pub struct CommitId([u8; COMMIT_ID_BYTES]);

impl CommitId {
    pub fn uninitialized() -> Self {
        CommitId([0; COMMIT_ID_BYTES])
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0[..]
    }

    pub const fn to_bytes(self) -> [u8; COMMIT_ID_BYTES] {
        self.0
    }

    pub const fn from_bytes(bytes: [u8; COMMIT_ID_BYTES]) -> Self {
        Self(bytes)
    }

    pub fn restore<P: AsRef<Path>>(path: P) -> Result<CommitId, Error> {
        let buf = fs::read(&path).map_err(RestoreError)?;
        let archived =
            rkyv::check_archived_root::<Self>(buf.as_slice()).unwrap();
        Ok(archived.deserialize(&mut rkyv::Infallible).unwrap())
    }

    pub fn persist<P: AsRef<Path>>(&self, path: P) -> Result<(), Error> {
        let buf = rkyv::to_bytes::<_, COMMIT_ID_BYTES>(&self.0).unwrap();
        fs::write(&path, &buf).map_err(PersistenceError)
    }
}

impl Hashable for CommitId {
    fn uninitialized() -> Self {
        CommitId([0; COMMIT_ID_BYTES])
    }

    fn as_slice(&self) -> &[u8] {
        &self.0[..]
    }
}

impl core::fmt::Debug for CommitId {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        if f.alternate() {
            write!(f, "0x")?
        }
        for byte in self.0 {
            write!(f, "{:02x}", &byte)?
        }
        Ok(())
    }
}

impl AsRef<[u8]> for CommitId {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<[u8; COMMIT_ID_BYTES]> for CommitId {
    fn from(bytes: [u8; COMMIT_ID_BYTES]) -> Self {
        Self::from_bytes(bytes)
    }
}

impl From<CommitId> for [u8; COMMIT_ID_BYTES] {
    fn from(commit: CommitId) -> Self {
        commit.to_bytes()
    }
}

#[derive(Debug, Archive, Serialize, Deserialize)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct SessionCommit {
    module_commit_ids: BTreeMap<ModuleId, ModuleCommitId>,
    id: CommitId,
}

impl SessionCommit {
    pub fn new() -> SessionCommit {
        SessionCommit {
            module_commit_ids: BTreeMap::new(),
            id: CommitId::uninitialized(),
        }
    }

    pub fn commit_id(&self) -> CommitId {
        self.id
    }

    pub fn add(
        &mut self,
        module_id: &ModuleId,
        module_commit: &ModuleCommit,
        bag: &mut ModuleCommitBag,
        memory_path: &MemoryPath,
    ) -> Result<(), Error> {
        if !self.module_commit_ids.contains_key(module_id) {
            self.module_commit_ids
                .insert(*module_id, module_commit.id());
        }
        bag.save_module_commit(module_commit, memory_path)
    }

    pub fn add_entry(
        &mut self,
        module_id: &ModuleId,
        module_commit_id: &ModuleCommitId,
    ) {
        self.module_commit_ids.insert(*module_id, *module_commit_id);
    }

    pub fn module_commit_ids(&self) -> &BTreeMap<ModuleId, ModuleCommitId> {
        &self.module_commit_ids
    }

    pub fn module_commit_ids_mut(
        &mut self,
    ) -> &mut BTreeMap<ModuleId, ModuleCommitId> {
        &mut self.module_commit_ids
    }

    pub fn calculate_id(&mut self) {
        let mut vec =
            Vec::from_iter(self.module_commit_ids().values().cloned());
        vec.sort();
        let root = Merkle::merkle(&mut vec).to_bytes();
        self.id = CommitId::from(root);
    }
}

impl Default for SessionCommit {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Archive, Serialize, Deserialize)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct SessionCommits {
    commits: BTreeMap<CommitId, SessionCommit>,
    current: CommitId,
    bags: BTreeMap<ModuleId, ModuleCommitBag>,
}

impl SessionCommits {
    pub fn new() -> Self {
        Self {
            commits: BTreeMap::new(),
            current: CommitId::uninitialized(),
            bags: BTreeMap::new(),
        }
    }

    pub fn from<P: AsRef<Path>>(path: P) -> Result<SessionCommits, Error> {
        if path.as_ref().exists() {
            SessionCommits::restore(path)
        } else {
            Ok(SessionCommits::new())
        }
    }

    pub fn set_current(&mut self, current: &CommitId) {
        self.current = *current;
    }

    pub fn add_and_set_current(
        &mut self,
        mut session_commit: SessionCommit,
    ) -> CommitId {
        // if previous current session commit contains module
        // for which the new session commit does not have an image
        // (meaning: the module has not been active in the closing session)
        // then enrich the image for it in the new current session commit
        // from the previous session commit and recalculate the id
        if let Some(current_session_commit) = self.get_current_session_commit()
        {
            let mut enriched = false;
            for (module_id, module_commit_id) in
                current_session_commit.module_commit_ids()
            {
                if !session_commit.module_commit_ids().contains_key(module_id) {
                    session_commit.add_entry(module_id, module_commit_id);
                    enriched = true;
                }
            }
            if enriched {
                session_commit.calculate_id();
            }
        }
        self.current = session_commit.commit_id();
        self.commits.insert(self.current, session_commit);
        self.current
    }

    pub fn get_session_commit(
        &self,
        session_commit_id: &CommitId,
    ) -> Option<&SessionCommit> {
        self.commits.get(session_commit_id)
    }

    pub fn get_session_commit_mut(
        &mut self,
        session_commit_id: &CommitId,
    ) -> Option<&mut SessionCommit> {
        self.commits.get_mut(session_commit_id)
    }

    pub fn get_current_session_commit(&self) -> Option<&SessionCommit> {
        self.commits.get(&self.current)
    }

    pub fn get_current_commit(&self) -> CommitId {
        self.current
    }

    pub fn with_every_session_commit<F>(&self, mut closure: F)
    where
        F: FnMut(&SessionCommit),
    {
        for (_commit_id, session_commit) in self.commits.iter() {
            closure(session_commit);
        }
    }

    pub fn get_bag_mut(
        &mut self,
        module_id: &ModuleId,
    ) -> &mut ModuleCommitBag {
        if !self.bags.contains_key(module_id) {
            self.bags.insert(*module_id, ModuleCommitBag::new());
        }
        self.bags.get_mut(module_id).unwrap()
    }

    pub fn get_bag(&self, module_id: &ModuleId) -> Option<&ModuleCommitBag> {
        self.bags.get(module_id)
    }
}

impl Persistable for SessionCommits {}

impl Default for SessionCommits {
    fn default() -> Self {
        Self::new()
    }
}
