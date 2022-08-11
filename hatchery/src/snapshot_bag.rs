// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use crate::error::Error;
use crate::snapshot::Snapshot;
use crate::snapshot::{MemoryPath, SnapshotId};
use crate::Error::SnapshotError;

#[derive(Debug)]
pub struct SnapshotBag {
    // first snapshot is always uncompressed
    ids: Vec<SnapshotId>,
    // we keep top uncompressed snapshot to make save snapshot efficient
    top: SnapshotId,
}

impl SnapshotBag {
    pub fn new() -> Self {
        Self {
            ids: Vec::new(),
            top: SnapshotId::random(),
        }
    }
    pub fn save_snapshot(
        &mut self,
        snapshot: &Snapshot,
        memory_path: &MemoryPath,
    ) -> Result<usize, Error> {
        snapshot.capture(memory_path)?;
        self.ids.push(snapshot.id());
        if self.ids.len() == 1 {
            // top is an uncompressed version of most recent snapshot
            Snapshot::from_id(self.top, memory_path)?.capture(memory_path)?;
            Ok(0)
        } else {
            let from_id =
                |snapshot_id| Snapshot::from_id(snapshot_id, memory_path);
            let top_snapshot = from_id(self.top)?;
            let accu_snapshot = from_id(SnapshotId::random())?;
            accu_snapshot.capture(snapshot)?;
            // accu and snapshot are both uncompressed
            // compressing snapshot against the top
            snapshot.capture_diff(&top_snapshot, memory_path)?;
            // snapshot is compressed but accu keeps the uncompressed copy
            // top is an uncompressed version of most recent snapshot
            top_snapshot.capture(&accu_snapshot)?;
            Ok(self.ids.len() - 1)
        }
    }
    pub fn restore_snapshot(
        &self,
        snapshot_index: usize,
        memory_path: &MemoryPath,
    ) -> Result<(), Error> {
        let is_valid = |index| index < self.ids.len();
        if !is_valid(snapshot_index) {
            return Err(SnapshotError(String::from("invalid snapshot index")));
        }
        let is_top = |index| (index + 1) == self.ids.len();
        let from_id = |snapshot_id| Snapshot::from_id(snapshot_id, memory_path);
        let final_snapshot = if snapshot_index == 0 {
            from_id(self.ids[0])?
        } else if is_top(snapshot_index) {
            from_id(self.top)?
        } else {
            let accu_snapshot = from_id(SnapshotId::random())?;
            accu_snapshot.capture(&from_id(self.ids[0])?)?;
            for i in 1..(snapshot_index + 1) {
                let snapshot = from_id(self.ids[i])?;
                snapshot
                    .decompress_and_patch(&accu_snapshot, &accu_snapshot)?;
            }
            accu_snapshot
        };
        final_snapshot.restore(memory_path)
    }
}
