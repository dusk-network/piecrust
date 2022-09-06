// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use uplink::ModuleId;

#[derive(Debug)]
struct CallData {
    module_id: ModuleId,
    limit: u64,
}

#[derive(Debug, Default)]
pub struct CallStack {
    inner: Vec<CallData>,
}

impl CallStack {
    /// Create a new call stack, with the initiating call being made to
    /// `module_id` with the given `limit`.
    pub fn new(module_id: ModuleId, limit: u64) -> Self {
        Self {
            inner: vec![CallData { module_id, limit }],
        }
    }

    /// Push a call onto the call stack.
    pub fn push(&mut self, module_id: ModuleId, limit: u64) {
        self.inner.push(CallData { module_id, limit })
    }

    /// Pop a call from the call stack.
    pub fn pop(&mut self) {
        if self.inner.len() > 1 {
            self.inner.pop();
        }
    }

    /// Return the `caller` of the currently running contract, may be
    /// uninitialized
    pub fn caller(&self) -> ModuleId {
        let len = self.inner.len();
        if len > 1 {
            self.inner[len - 2].module_id
        } else {
            ModuleId::uninitialized()
        }
    }

    /// Return the point limit given to the currently executing contract
    pub fn limit(&self) -> u64 {
        self.inner[self.inner.len() - 1].limit
    }
}
