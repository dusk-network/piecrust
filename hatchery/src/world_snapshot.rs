// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::collections::BTreeMap;

use dallo::ModuleId;

use crate::error::Error;
use crate::snapshot::{MemoryPath, SnapshotId};
use crate::world::World;
use rkyv::{Archive, Deserialize, Serialize};

pub const WORLD_SNAPSHOT_ID_BYTES: usize = 32;
#[derive(
    Debug,
    PartialEq,
    Eq,
    Archive,
    Serialize,
    Deserialize,
    PartialOrd,
    Ord,
    Hash,
    Clone,
    Copy,
)]
pub struct WorldSnapshotId([u8; WORLD_SNAPSHOT_ID_BYTES]);
impl WorldSnapshotId {
    pub const fn uninitialized() -> Self {
        WorldSnapshotId([0u8; WORLD_SNAPSHOT_ID_BYTES])
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
    pub fn add(&mut self, snapshot_id: &SnapshotId) {
        let p = snapshot_id.as_bytes().as_ptr();
        for (i, b) in self.0.iter_mut().enumerate() {
            *b ^= unsafe { *p.add(i) };
        }
    }
}
impl From<[u8; 32]> for WorldSnapshotId {
    fn from(array: [u8; 32]) -> Self {
        WorldSnapshotId(array)
    }
}

#[derive(Debug)]
pub struct WorldSnapshot {
    id: WorldSnapshotId,
    snapshot_indices: BTreeMap<ModuleId, usize>,
}

impl WorldSnapshot {
    pub fn new() -> Self {
        Self {
            id: WorldSnapshotId::uninitialized(),
            snapshot_indices: BTreeMap::new(),
        }
    }
    pub fn add(&mut self, module_id: ModuleId, snapshot_index: usize) {
        self.snapshot_indices.insert(module_id, snapshot_index);
    }
    pub fn finalize_id(&mut self, world_snapshot_id: WorldSnapshotId) {
        self.id = world_snapshot_id
    }
    pub fn restore_snapshots(&self, world: &World) -> Result<(), Error> {
        for (module_id, snapshot_index) in self.snapshot_indices.iter() {
            let memory_path = MemoryPath::new(world.memory_path(module_id));
            world.restore_snapshot_with_index(
                module_id,
                *snapshot_index,
                &memory_path,
            )?;
        }
        Ok(())
    }
}
