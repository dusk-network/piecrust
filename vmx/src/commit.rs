// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

// use std::fmt::{Display, Formatter};

use uplink::ModuleId;

use std::collections::BTreeMap;

use crate::error::Error::{self, SessionError};

pub const COMMIT_ID_BYTES: usize = 32;

#[derive(Clone, Copy, PartialOrd, Ord, PartialEq, Eq)]
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

#[derive(Clone, Copy, Default, PartialOrd, Ord, PartialEq, Eq)]
pub struct SessionCommitId([u8; COMMIT_ID_BYTES]);

impl SessionCommitId {
    pub fn uninitialized() -> Self {
        SessionCommitId([0; COMMIT_ID_BYTES])
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

impl core::fmt::Debug for SessionCommitId {
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

pub struct SessionCommit {
    ids: BTreeMap<ModuleId, ModuleCommitId>,
    id: SessionCommitId,
}

impl SessionCommit {
    pub fn new() -> SessionCommit {
        SessionCommit {
            ids: BTreeMap::new(),
            id: SessionCommitId::uninitialized(),
        }
    }

    pub fn commit_id(&self) -> SessionCommitId {
        self.id
    }

    pub fn add(&mut self, module_id: &ModuleId, commit_id: &ModuleCommitId) {
        self.ids.insert(*module_id, *commit_id);
        self.id.add(commit_id);
    }

    pub fn ids(&self) -> &BTreeMap<ModuleId, ModuleCommitId> {
        &self.ids
    }

    pub fn get(&self, module_id: &ModuleId) -> Option<&ModuleCommitId> {
        self.ids.get(module_id)
    }
}

impl Default for SessionCommit {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SessionCommits(BTreeMap<SessionCommitId, SessionCommit>);

impl SessionCommits {
    pub fn new() -> SessionCommits {
        SessionCommits(BTreeMap::new())
    }

    pub fn add(&mut self, session_commit: SessionCommit) {
        self.0.insert(session_commit.commit_id(), session_commit);
    }

    pub fn get_session_commit(
        &self,
        session_commit_id: &SessionCommitId,
    ) -> Option<&SessionCommit> {
        self.0.get(session_commit_id)
    }

    pub fn with_every_module_commit<F>(
        &self,
        session_commit_id: &SessionCommitId,
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
}

impl Default for SessionCommits {
    fn default() -> Self {
        Self::new()
    }
}
