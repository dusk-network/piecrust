// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust_uplink::ModuleId;

pub struct Event {
    source: ModuleId,
    data: Vec<u8>,
}

impl Event {
    pub(crate) fn new(source: ModuleId, data: Vec<u8>) -> Self {
        Self { source, data }
    }

    pub fn source(&self) -> ModuleId {
        self.source
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }
}
