// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::marker::PhantomData;
use std::mem;

use piecrust_uplink::ContractId;

#[derive(Debug, Clone, Copy)]
pub struct StackElement {
    pub contract_id: ContractId,
    pub limit: u64,
    pub mem_len: usize,
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
    pub fn push(&mut self, elem: StackElement) {
        self.tree.push(elem);
        self.stack.push(elem);
    }

    /// Pops an element from the callstack.
    pub fn pop(&mut self) -> Option<StackElement> {
        self.tree.pop();
        self.stack.pop()
    }

    /// Pops an element from the callstack and prunes the call tree.
    pub fn pop_prune(&mut self) -> Option<StackElement> {
        self.tree.pop_prune();
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
    pub fn iter_tree(&self) -> CallTreeIter {
        CallTreeIter {
            node: self.tree.0,
            _marker: PhantomData,
        }
    }
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

    /// Clears the tree under the current node, and moves to the previous node.
    fn pop_prune(&mut self) {
        self.0 = self.0.and_then(|inner| unsafe {
            let prev = (*inner).prev;
            if let Some(prev) = prev {
                (*prev).children.pop();
            }
            free_tree(inner);
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

/// An iterator over a [`CallTree`].
///
/// It starts at the righmost node and proceeds leftward towards its siblings,
/// up toward the root.
pub struct CallTreeIter<'a> {
    node: Option<*mut CallTreeInner>,
    _marker: PhantomData<&'a ()>,
}

impl<'a> Iterator for CallTreeIter<'a> {
    type Item = &'a StackElement;

    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            let node = match self.node {
                Some(node) => node,
                None => return None,
            };

            let elem = &(*node).elem;

            self.node = (*node).prev.map(|prev_node| {
                let node_index = (*prev_node)
                    .children
                    .iter()
                    .position(|&n| n == node)
                    .expect("The child must be the prev's child");

                if node_index == 0 {
                    prev_node
                } else {
                    let sibling_index = node_index - 1;
                    let mut next_node = (*prev_node).children[sibling_index];

                    while !(*next_node).children.is_empty() {
                        let last_index = (*next_node).children.len() - 1;
                        next_node = (*next_node).children[last_index];
                    }

                    next_node
                }
            });

            Some(elem)
        }
    }
}
