// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

mod module_commit;
mod module_commit_store;
mod module_commit_bag;

use bytecheck::CheckBytes;
use rkyv::{Archive, Deserialize, Serialize};

use piecrust_uplink::ModuleId;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use rand::Rng;

use crate::error::Error::{self, PersistenceError, RestoreError, SessionError};
use crate::merkle::Merkle;
use crate::persistable::Persistable;

pub use module_commit::{ModuleCommit, ModuleCommitLike};
pub use module_commit_store::ModuleCommitStore;
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
        ModuleCommitId(
            rand::thread_rng().gen::<[u8; COMMIT_ID_BYTES]>(),
        )
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
    ids: BTreeMap<ModuleId, ModuleCommitId>,
    id: CommitId,
}

impl SessionCommit {
    pub fn new() -> SessionCommit {
        SessionCommit {
            ids: BTreeMap::new(),
            id: CommitId::uninitialized(),
        }
    }

    pub fn commit_id(&self) -> CommitId {
        self.id
    }

    pub fn add(&mut self, module_id: &ModuleId, commit_id: &ModuleCommitId) {
        self.ids.insert(*module_id, *commit_id);
    }

    pub fn ids(&self) -> &BTreeMap<ModuleId, ModuleCommitId> {
        &self.ids
    }

    pub fn calculate_id(&mut self) {
        let mut vec = Vec::from_iter(self.ids().values().cloned());
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
pub struct SessionCommits(BTreeMap<CommitId, SessionCommit>);

impl SessionCommits {
    pub fn new() -> SessionCommits {
        SessionCommits(BTreeMap::new())
    }

    pub fn from<P: AsRef<Path>>(path: P) -> Result<SessionCommits, Error> {
        if path.as_ref().exists() {
            SessionCommits::restore(path)
        } else {
            Ok(SessionCommits::new())
        }
    }

    pub fn add(&mut self, session_commit: SessionCommit) {
        self.0.insert(session_commit.commit_id(), session_commit);
    }

    pub fn get_session_commit(
        &self,
        session_commit_id: &CommitId,
    ) -> Option<&SessionCommit> {
        self.0.get(session_commit_id)
    }

    pub fn with_every_module_commit<F>(
        &self,
        commit_id: &CommitId,
        closure: F,
    ) -> Result<(), Error>
    where
        F: Fn(&ModuleId, &ModuleCommitId) -> Result<(), Error>,
    {
        match self.get_session_commit(commit_id) {
            Some(session_commit) => {
                for (module_id, module_commit_id) in session_commit.ids().iter()
                {
                    closure(module_id, module_commit_id)?;
                }
                Ok(())
            }
            None => Err(SessionError("unknown session commit id".into())),
        }
    }

    pub fn with_every_session_commit<F>(&self, mut closure: F)
    where
        F: FnMut(&SessionCommit),
    {
        for (_commit_id, session_commit) in self.0.iter() {
            closure(session_commit);
        }
    }
}

impl Persistable for SessionCommits {}
impl Default for SessionCommits {
    fn default() -> Self {
        Self::new()
    }
}
