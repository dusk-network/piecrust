// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use bytecheck::CheckBytes;
use rkyv::{
    ser::serializers::{BufferScratch, CompositeSerializer, WriteSerializer},
    ser::Serializer,
    Archive, Deserialize, Serialize,
};

use uplink::ModuleId;

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::ptr;

use crate::error::Error::{self, PersistenceError, SessionError};

pub const COMMIT_ID_BYTES: usize = 32;
const SESSION_COMMITS_SCRATCH_SIZE: usize = 64;

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
    pub fn from(mem: &[u8]) -> Result<Self, Error> {
        Ok(ModuleCommitId(*blake3::hash(mem).as_bytes()))
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0[..]
    }
}

impl From<[u8; COMMIT_ID_BYTES]> for ModuleCommitId {
    fn from(array: [u8; COMMIT_ID_BYTES]) -> Self {
        ModuleCommitId(array)
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

    fn add(&mut self, module_commit_id: &ModuleCommitId) {
        let mut hasher = blake3::Hasher::new();
        hasher.update(self.0.as_slice());
        hasher.update(module_commit_id.as_bytes());
        self.0 = *hasher.finalize().as_bytes();
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
        self.id.add(commit_id);
    }

    pub fn ids(&self) -> &BTreeMap<ModuleId, ModuleCommitId> {
        &self.ids
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
            SessionCommits::read(path)
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
        session_commit_id: &CommitId,
        closure: F,
    ) -> Result<(), Error>
    where
        F: Fn(&ModuleId, &ModuleCommitId) -> Result<(), Error>,
    {
        match self.get_session_commit(session_commit_id) {
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

    pub fn read<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let file =
            std::fs::File::open(path.as_ref()).map_err(PersistenceError)?;
        let metadata =
            std::fs::metadata(path.as_ref()).map_err(PersistenceError)?;
        let ptr = unsafe {
            libc::mmap(
                ptr::null_mut(),
                u32::MAX as usize,
                libc::PROT_READ,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            ) as *mut u8
        };
        let slice = unsafe {
            core::slice::from_raw_parts(ptr, metadata.len() as usize)
        };
        let archived = rkyv::check_archived_root::<Self>(slice).unwrap();
        Ok(archived.deserialize(&mut rkyv::Infallible).unwrap())
    }

    pub fn write<P: AsRef<Path>>(&self, path: P) -> Result<(), Error> {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .map_err(PersistenceError)?;

        let mut scratch_buf = [0u8; SESSION_COMMITS_SCRATCH_SIZE];
        let scratch = BufferScratch::new(&mut scratch_buf);
        let serializer = WriteSerializer::new(file);
        let mut composite =
            CompositeSerializer::new(serializer, scratch, rkyv::Infallible);

        composite.serialize_value(self).unwrap();
        Ok(())
    }
}

impl Default for SessionCommits {
    fn default() -> Self {
        Self::new()
    }
}
