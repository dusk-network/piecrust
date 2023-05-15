// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust_uplink::ContractId;

#[derive(Debug, Default)]
pub struct CallStack {
    stack: Vec<StackElement>,
}

impl CallStack {
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the length of the call stack.
    pub fn len(&self) -> usize {
        self.stack.len()
    }

    /// Push an element to the call stack.
    ///
    /// # Panics
    /// If an instance of the given contract ID is absent from the stack.
    pub fn push(&mut self, contract_id: ContractId, limit: u64) {
        self.stack.push(StackElement { contract_id, limit });
    }

    /// Pops an element from the callstack.
    pub fn pop(&mut self) -> Option<StackElement> {
        self.stack.pop()
    }

    /// Returns a view of the stack to the `n`th element from the top.
    ///
    /// # Safety
    /// The reference to the instance available in the returned element is only
    /// guaranteed to be valid before the stack is called.
    pub fn nth_from_top(&self, n: usize) -> Option<StackElement> {
        let len = self.stack.len();

        if len > n {
            let elem = &self.stack[len - (n + 1)];
            Some(StackElement {
                contract_id: elem.contract_id,
                limit: elem.limit,
            })
        } else {
            None
        }
    }
}

#[derive(Debug)]
pub struct StackElement {
    pub contract_id: ContractId,
    pub limit: u64,
}
