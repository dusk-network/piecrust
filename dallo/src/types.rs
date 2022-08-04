// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use rkyv::{Archive, Deserialize, Serialize};

pub const MODULE_ID_BYTES: usize = 32;

#[derive(
    Debug,
    PartialEq,
    Eq,
    Archive,
    Serialize,
    Deserialize,
    PartialOrd,
    Ord,
    Clone,
    Copy,
)]
#[repr(C)]
pub struct ModuleId([u8; MODULE_ID_BYTES]);

impl ModuleId {
    pub const fn uninitialized() -> Self {
        ModuleId([0u8; MODULE_ID_BYTES])
    }

    pub(crate) fn as_ptr(&self) -> *const u8 {
        self.0.as_ptr()
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}

impl From<[u8; 32]> for ModuleId {
    fn from(array: [u8; 32]) -> Self {
        ModuleId(array)
    }
}
