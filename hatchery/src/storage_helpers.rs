// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use crate::snapshot::SnapshotId;
use dallo::{ModuleId, MODULE_ID_BYTES};

pub fn combine_module_snapshot_names(
    module_name: impl AsRef<str>,
    snapshot_name: impl AsRef<str>,
) -> String {
    format!("{}_{}", module_name.as_ref(), snapshot_name.as_ref())
}

pub fn module_id_to_name(module_id: ModuleId) -> String {
    format!("{}", ByteArrayWrapper(module_id))
}

pub fn snapshot_id_to_name(snapshot_id: SnapshotId) -> String {
    format!("{}", ByteArrayWrapper(snapshot_id))
}

struct ByteArrayWrapper(pub [u8; MODULE_ID_BYTES]);

impl core::fmt::UpperHex for ByteArrayWrapper {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let bytes = &self.0[..];
        if f.alternate() {
            write!(f, "0x")?
        }
        for byte in bytes {
            write!(f, "{:02X}", &byte)?
        }
        Ok(())
    }
}

impl core::fmt::Display for ByteArrayWrapper {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::UpperHex::fmt(self, f)
    }
}
