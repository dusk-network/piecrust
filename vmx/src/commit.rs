// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

// use std::fmt::{Display, Formatter};

use uplink::ModuleId;

use rand::prelude::*;
use std::collections::BTreeMap;

pub const COMMIT_ID_BYTES: usize = 4;

#[derive(Clone, Copy, PartialOrd, Ord, PartialEq, Eq)]
pub struct ModuleCommitId([u8; COMMIT_ID_BYTES]);

impl ModuleCommitId {
    pub fn new() -> ModuleCommitId {
        ModuleCommitId(thread_rng().gen::<[u8; COMMIT_ID_BYTES]>())
    }

    pub fn uninitialized() -> ModuleCommitId {
        ModuleCommitId([0; COMMIT_ID_BYTES])
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

impl Default for ModuleCommitId {
    fn default() -> Self {
        Self::new()
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

// SessionCommitId is physically the same as ModuleCommitId
// yet semantically it applies to aggregates
#[derive(Clone, Copy, Default, PartialOrd, Ord, PartialEq, Eq)]
pub struct SessionCommitId([u8; COMMIT_ID_BYTES]);

impl SessionCommitId {
    pub fn uninitialized() -> Self {
        SessionCommitId([0; COMMIT_ID_BYTES])
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0[..]
    }

    pub fn add(&mut self, module_commit_id: &ModuleCommitId) {
        let p = module_commit_id.as_bytes().as_ptr();
        for (i, b) in self.0.iter_mut().enumerate() {
            *b ^= unsafe { *p.add(i) };
        }
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

#[derive(Clone, Default, PartialOrd, Ord, PartialEq, Eq)]
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

#[derive(Clone, PartialOrd, Ord, PartialEq, Eq)]
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
}

impl Default for SessionCommits {
    fn default() -> Self {
        Self::new()
    }
}
