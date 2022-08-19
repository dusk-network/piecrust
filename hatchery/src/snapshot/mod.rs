// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

mod diff_data;
mod module_snapshot;
mod module_snapshot_bag;

pub use module_snapshot::{MemoryPath, ModuleSnapshot, ModuleSnapshotId};
pub use module_snapshot_bag::ModuleSnapshotBag;

use std::collections::BTreeMap;

use dallo::ModuleId;

use crate::error::Error;
use crate::instance::Instance;
use rkyv::{Archive, Deserialize, Serialize};

pub const SNAPSHOT_ID_BYTES: usize = 32;
/// Snapshot of the world encompassing states of all world's modules.
#[derive(
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
pub struct SnapshotId([u8; SNAPSHOT_ID_BYTES]);
impl SnapshotId {
    pub const fn uninitialized() -> Self {
        SnapshotId([0u8; SNAPSHOT_ID_BYTES])
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
    pub fn add(&mut self, module_snapshot_id: &ModuleSnapshotId) {
        let p = module_snapshot_id.as_bytes().as_ptr();
        for (i, b) in self.0.iter_mut().enumerate() {
            *b ^= unsafe { *p.add(i) };
        }
    }
}

impl From<[u8; 32]> for SnapshotId {
    fn from(array: [u8; 32]) -> Self {
        SnapshotId(array)
    }
}

impl core::fmt::Debug for SnapshotId {
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

#[derive(Debug)]
pub struct Snapshot {
    id: SnapshotId,
    module_snapshot_indices: BTreeMap<ModuleId, usize>,
}

impl Snapshot {
    pub(crate) fn new() -> Self {
        Self {
            id: SnapshotId::uninitialized(),
            module_snapshot_indices: BTreeMap::new(),
        }
    }

    pub(crate) fn persist_module_snapshot(
        &mut self,
        memory_path: &MemoryPath,
        instance: &mut Instance,
        module_id: &ModuleId,
    ) -> Result<(), Error> {
        let module_snapshot = ModuleSnapshot::new(
            memory_path,
            instance.arg_buffer_span(),
            instance.heap_base(),
        )?;
        let module_snapshot_index = instance
            .module_snapshot_bag_mut()
            .save_module_snapshot(&module_snapshot, memory_path)?;
        self.id.add(&module_snapshot.id());
        self.module_snapshot_indices
            .insert(*module_id, module_snapshot_index);
        Ok(())
    }

    pub(crate) fn restore_module_snapshots<'a, F1, F2>(
        &self,
        get_memory_path: F1,
        get_instance: F2,
    ) -> Result<(), Error>
    where
        F1: Fn(ModuleId) -> MemoryPath,
        F2: Fn(ModuleId) -> &'a Instance,
    {
        for (module_id, module_snapshot_index) in
            self.module_snapshot_indices.iter()
        {
            let memory_path = get_memory_path(*module_id);
            get_instance(*module_id)
                .module_snapshot_bag()
                .restore_module_snapshot(
                    *module_snapshot_index,
                    &memory_path,
                )?;
        }
        Ok(())
    }

    pub fn id(&self) -> SnapshotId {
        self.id
    }
}
