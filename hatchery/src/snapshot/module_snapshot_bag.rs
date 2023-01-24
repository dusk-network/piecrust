// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use crate::error::Error;
use crate::snapshot::{MemoryPath, ModuleSnapshot, ModuleSnapshotId};
use crate::Error::SnapshotError;

#[derive(Debug)]
pub struct ModuleSnapshotBag {
    // first module snapshot is always uncompressed
    ids: Vec<ModuleSnapshotId>,
    // we keep top uncompressed module snapshot to make saving module snapshots
    // efficient
    top: ModuleSnapshotId,
}

impl ModuleSnapshotBag {
    pub fn new() -> Self {
        Self {
            ids: Vec::new(),
            top: ModuleSnapshotId::random(),
        }
    }

    pub(crate) fn save_module_snapshot(
        &mut self,
        module_snapshot: &ModuleSnapshot,
        memory_path: &MemoryPath,
    ) -> Result<(), Error> {
        module_snapshot.capture(memory_path)?;
        self.ids.push(module_snapshot.id());
        if self.ids.len() == 1 {
            // top is an uncompressed version of most recent snapshot
            ModuleSnapshot::from_id(self.top, memory_path)?
                .capture(memory_path)?;
            Ok(())
        } else {
            let from_id = |module_snapshot_id| {
                ModuleSnapshot::from_id(module_snapshot_id, memory_path)
            };
            let top_snapshot = from_id(self.top)?;
            let accu_snapshot = from_id(ModuleSnapshotId::random())?;
            accu_snapshot.capture(module_snapshot)?;
            // accu and snapshot are both uncompressed
            // compressing snapshot against the top
            module_snapshot.capture_diff(&top_snapshot, memory_path)?;
            // snapshot is compressed but accu keeps the uncompressed copy
            // top is an uncompressed version of most recent snapshot
            top_snapshot.capture(&accu_snapshot)?;
            Ok(())
        }
    }

    pub(crate) fn restore_module_snapshot(
        &self,
        source_module_snapshot_id: ModuleSnapshotId,
        memory_path: &MemoryPath,
    ) -> Result<(), Error> {
        // let is_valid = |index| index < self.ids.len();
        // if !is_valid(module_snapshot_index) {
        //     return Err(SnapshotError("invalid snapshot index".into()));
        // }
        // let is_top = |index| (index + 1) == self.ids.len();
        let from_id = |module_snapshot_id| {
            ModuleSnapshot::from_id(module_snapshot_id, memory_path)
        };
        let final_snapshot = if source_module_snapshot_id == self.ids[0] {
            from_id(self.ids[0])?
        } else if source_module_snapshot_id == self.top {
            from_id(self.top)?
        } else {
            let accu_snapshot = from_id(ModuleSnapshotId::random())?;
            accu_snapshot.capture(&from_id(self.ids[0])?)?;
            for i in 1..self.ids.len() {
                let snapshot_id = self.ids[i];
                let snapshot = from_id(snapshot_id)?;
                snapshot
                    .decompress_and_patch(&accu_snapshot, &accu_snapshot)?;
                if source_module_snapshot_id == snapshot_id {
                    break;
                }
            }
            accu_snapshot
        };
        final_snapshot.restore(memory_path)
    }
}
