// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use dallo::ModuleId;

#[derive(Debug)]
struct CallData {
    module_id: ModuleId,
}

#[derive(Debug, Default)]
pub struct CallStack {
    block_height: u64,
    inner: Vec<CallData>,
}

impl CallStack {
    /// Create a new call stack, with the initiating call being made to
    /// `module_id` and the given `block_height`.
    pub fn new(block_height: u64, module_id: ModuleId) -> Self {
        Self {
            block_height,
            inner: vec![CallData { module_id }],
        }
    }

    /// Push a call onto the call stack.
    pub fn push(&mut self, module_id: ModuleId) {
        self.inner.push(CallData { module_id })
    }

    /// Pop a call from the call stack.
    pub fn pop(&mut self) {
        if self.inner.len() > 1 {
            self.inner.pop();
        }
    }

    /// Return the `caller` of the currently running contract, if it is not the
    /// first call. Otherwise return `None`.
    pub fn caller(&self) -> Option<ModuleId> {
        let len = self.inner.len();

        if len > 1 {
            let module_id = self.inner[len - 2].module_id;
            return Some(module_id);
        }

        None
    }

    /// Returns the height used to create this stack.
    pub fn block_height(&self) -> u64 {
        self.block_height
    }
}
