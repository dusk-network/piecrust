// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use dallo::ModuleId;

/// Stack of the calls being performed by the virtual machine.
#[derive(Debug)]
pub struct CallStack {
    elems: Vec<StackElement>,
}

#[derive(Debug)]
struct StackElement {
    module_id: ModuleId,
}

impl CallStack {
    /// Create a new call stack starting at the given module ID.
    pub fn new(module_id: ModuleId) -> Self {
        Self {
            elems: vec![StackElement { module_id }],
        }
    }

    /// Push the given module id onto the call stack.
    pub fn push(&mut self, module_id: ModuleId) {
        self.elems.push(StackElement { module_id })
    }

    /// Pop an element off the stack.
    ///
    /// # Panics
    /// When there is only a single element left in the stack.
    pub fn pop(&mut self) -> ModuleId {
        if self.elems.len() == 1 {
            panic!("tried to pop the last element in the call stack");
        }

        self.elems
            .pop()
            .expect("there should always be an element to pop")
            .module_id
    }

    /// The caller of the contract currently being executed. This is `None` if
    /// it is the initial call.
    pub fn caller(&self) -> Option<ModuleId> {
        match self.elems.len() == 1 {
            true => None,
            false => {
                let index = self.elems.len() - 2;
                Some(self.elems[index].module_id)
            }
        }
    }

    /// The contract currently being executed.
    pub fn callee(&self) -> ModuleId {
        let index = self.elems.len() - 1;
        self.elems[index].module_id
    }
}
