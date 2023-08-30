// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::mem;

use piecrust_uplink::ContractId;

#[derive(Debug, Clone, Copy)]
pub struct StackElement {
    pub contract_id: ContractId,
    pub limit: u64,
}

/// A stack of contract calls.
#[derive(Debug, Default)]
pub struct CallStack {
    stack: Vec<StackElement>,
    tree: CallTree,
}

impl CallStack {
    pub const fn new() -> Self {
        Self {
            stack: Vec::new(),
            tree: CallTree::new(),
        }
    }

    /// Push an element to the call stack.
    pub fn push(&mut self, contract_id: ContractId, limit: u64) {
        let se = StackElement { contract_id, limit };
        self.tree.push(se);
        self.stack.push(se);
    }

    /// Pops an element from the callstack.
    pub fn pop(&mut self) -> Option<StackElement> {
        self.tree.pop();
        self.stack.pop()
    }

    /// Returns a view of the stack to the `n`th element from the top.
    pub fn nth_from_top(&self, n: usize) -> Option<StackElement> {
        let len = self.stack.len();

        if len > n {
            Some(self.stack[len - (n + 1)])
        } else {
            None
        }
    }

    /// Clear the call stack of all elements.
    pub fn clear(&mut self) {
        self.tree.clear();
        self.stack.clear();
    }

    /// Returns an iterator over the call tree, starting from the rightmost
    /// leaf, and proceeding to the top of the current position of the tree.
    pub fn iter_tree(&self) -> impl Iterator<Item = StackElement> {
        let mut v = Vec::new();
        if let Some(inner) = self.tree.0 {
            right_fill(&mut v, unsafe { &*inner });
        }
        v.into_iter()
    }
}

fn right_fill(v: &mut Vec<StackElement>, tree: &CallTreeInner) {
    for child in tree.children.iter().rev() {
        right_fill(v, unsafe { &**child });
    }
    v.push(tree.elem);
}

#[derive(Debug, Default)]
struct CallTree(Option<*mut CallTreeInner>);

impl CallTree {
    /// Creates a new empty call tree, starting with at the given contract.
    const fn new() -> Self {
        Self(None)
    }

    /// Pushes a new child to the current node, and advances to it.
    fn push(&mut self, elem: StackElement) {
        match self.0 {
            None => self.0 = Some(CallTreeInner::new(elem)),
            Some(inner) => unsafe {
                let node = CallTreeInner::with_prev(elem, inner);
                (*inner).children.push(node);
                self.0 = Some(node)
            },
        }
    }

    /// Moves to the previous node.
    fn pop(&mut self) {
        self.0 = self.0.and_then(|inner| unsafe {
            let prev = (*inner).prev;
            if prev.is_none() {
                free_tree(inner);
            }
            prev
        });
    }

    fn clear(&mut self) {
        while self.0.is_some() {
            self.pop();
        }
    }
}

impl Drop for CallTree {
    fn drop(&mut self) {
        self.clear()
    }
}

unsafe fn free_tree(root: *mut CallTreeInner) {
    let mut node = Box::from_raw(root);

    let mut children = Vec::new();
    mem::swap(&mut node.children, &mut children);

    for child in children {
        free_tree(child);
    }
}

struct CallTreeInner {
    elem: StackElement,
    children: Vec<*mut Self>,
    prev: Option<*mut Self>,
}

impl CallTreeInner {
    fn new(elem: StackElement) -> *mut Self {
        Box::leak(Box::new(Self {
            elem,
            children: Vec::new(),
            prev: None,
        }))
    }

    fn with_prev(elem: StackElement, prev: *mut Self) -> *mut Self {
        Box::leak(Box::new(Self {
            elem,
            children: Vec::new(),
            prev: Some(prev),
        }))
    }
}
