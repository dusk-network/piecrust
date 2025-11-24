// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::fmt;
use std::marker::PhantomData;
use std::mem;

use piecrust_uplink::ContractId;

/// An element of the call tree.
#[derive(Debug, Clone, Copy)]
pub struct CallTreeElem {
    pub contract_id: ContractId,
    pub limit: u64,
    pub spent: u64,
    pub mem_len: usize,
}

/// The tree of contract calls.
#[derive(Default)]
pub struct CallTree(Option<*mut CallTreeNode>);

impl CallTree {
    /// Creates a new empty call tree, starting with the given contract.
    pub(crate) const fn new() -> Self {
        Self(None)
    }

    /// Push an element to the call tree.
    ///
    /// This pushes a new child to the current node, and advances to it.
    pub(crate) fn push(&mut self, elem: CallTreeElem) {
        match self.0 {
            None => self.0 = Some(CallTreeNode::new(elem)),
            Some(inner) => unsafe {
                let node = CallTreeNode::with_parent(elem, inner);
                (*inner).children.push(node);
                self.0 = Some(node)
            },
        }
    }

    /// Moves to the parent node and set the gas spent of the current element,
    /// returning it.
    pub(crate) fn move_up(&mut self, spent: u64) -> Option<CallTreeElem> {
        self.0.map(|inner| unsafe {
            (*inner).elem.spent = spent;
            let elem = (*inner).elem;

            let parent = (*inner).parent;
            if parent.is_none() {
                free_tree(inner);
            }
            self.0 = parent;

            elem
        })
    }

    /// Moves to the parent node, clearing the tree under it, and returns the
    /// current element.
    pub(crate) fn move_up_prune(&mut self) -> Option<CallTreeElem> {
        self.0.map(|inner| unsafe {
            let elem = (*inner).elem;

            let parent = (*inner).parent;
            if let Some(parent) = parent {
                (*parent).children.pop();
            }
            free_tree(inner);
            self.0 = parent;

            elem
        })
    }

    /// Give the current node the amount spent and recursively update amount
    /// spent to accurately reflect what each node spent during each call.
    pub(crate) fn update_spent(&mut self, spent: u64) {
        if let Some(inner) = self.0 {
            unsafe {
                (*inner).elem.spent = spent;
                update_spent(inner);
            }
        }
    }

    /// Returns the `n`th parent element counting from the current node. The
    /// zeroth parent element is the current node.
    pub(crate) fn nth_parent(&self, n: usize) -> Option<CallTreeElem> {
        let mut current = self.0;

        let mut i = 0;
        while i < n {
            current = current.and_then(|inner| unsafe { (*inner).parent });
            i += 1;
        }

        current.map(|inner| unsafe { (*inner).elem })
    }

    /// Returns all call ids.
    pub(crate) fn call_ids(&self) -> Vec<&ContractId> {
        let mut v = Vec::new();
        let mut current = self.0;

        while current.is_some() {
            let p = *current.as_ref().unwrap();
            v.push(unsafe { &(*p).elem.contract_id });
            current = current.and_then(|inner| unsafe { (*inner).parent });
        }

        v
    }

    /// Clears the call tree of all elements.
    pub(crate) fn clear(&mut self) {
        unsafe {
            if let Some(inner) = self.0 {
                let mut root = inner;

                while let Some(parent) = (*root).parent {
                    root = parent;
                }

                self.0 = None;
                free_tree(root);
            }
        }
    }

    /// Returns an iterator over the call tree, starting from the rightmost
    /// leaf, and proceeding to the top of the current position of the tree.
    pub fn iter(&self) -> impl Iterator<Item = &CallTreeElem> {
        CallTreeIter {
            tree: self.0.map(|root| unsafe {
                let mut node = root;

                while !(*node).children.is_empty() {
                    let last_index = (*node).children.len() - 1;
                    node = (*node).children[last_index];
                }

                Subtree { root, node }
            }),
            _marker: PhantomData,
        }
    }
}

struct Subtree {
    root: *mut CallTreeNode,
    node: *mut CallTreeNode,
}

impl Drop for CallTree {
    fn drop(&mut self) {
        self.clear()
    }
}

impl fmt::Display for CallTree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            None => write!(f, "[]"),
            Some(root) => unsafe {
                // Find the actual root of the tree
                let mut actual_root = root;
                while let Some(parent) = (*actual_root).parent {
                    actual_root = parent;
                }

                // Format the tree from the root
                format_node(f, actual_root, root)
            },
        }
    }
}

impl fmt::Debug for CallTree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            None => write!(f, "[]"),
            Some(root) => unsafe {
                // Find the actual root of the tree
                let mut actual_root = root;
                while let Some(parent) = (*actual_root).parent {
                    actual_root = parent;
                }

                // Format the tree with pretty printing
                format_node_pretty(f, actual_root, root, "", true, true)
            },
        }
    }
}

/// Helper function to format a node and its children recursively
unsafe fn format_node(
    f: &mut fmt::Formatter<'_>,
    node: *mut CallTreeNode,
    cursor: *mut CallTreeNode,
) -> fmt::Result {
    let node_ref = &*node;
    let is_cursor = node == cursor;

    // Format contract ID (first 4 bytes for brevity)
    let id_bytes = node_ref.elem.contract_id.to_bytes();
    let short_id = format!(
        "{:02x}{:02x}{:02x}{:02x}",
        id_bytes[0], id_bytes[1], id_bytes[2], id_bytes[3]
    );

    if is_cursor {
        write!(f, "*0x{}", short_id)?;
    } else {
        write!(f, "0x{}", short_id)?;
    }

    // If there are children, format them in brackets
    if !node_ref.children.is_empty() {
        write!(f, "[")?;
        for (i, &child) in node_ref.children.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            format_node(f, child, cursor)?;
        }
        write!(f, "]")?;
    }

    Ok(())
}

/// Helper function to format a node and its children in a tree structure
unsafe fn format_node_pretty(
    f: &mut fmt::Formatter<'_>,
    node: *mut CallTreeNode,
    cursor: *mut CallTreeNode,
    prefix: &str,
    is_root: bool,
    is_last: bool,
) -> fmt::Result {
    let node_ref = &*node;
    let is_cursor = node == cursor;

    // Format contract ID (first 4 bytes for brevity)
    let id_bytes = node_ref.elem.contract_id.to_bytes();
    let short_id = format!(
        "{:02x}{:02x}{:02x}{:02x}",
        id_bytes[0], id_bytes[1], id_bytes[2], id_bytes[3]
    );

    if !is_root {
        write!(f, "{}", prefix)?;
        write!(f, "{}", if is_last { "└── " } else { "├── " })?;
    }

    if is_cursor {
        writeln!(f, "*0x{}", short_id)?;
    } else {
        writeln!(f, "0x{}", short_id)?;
    }

    // Format children
    let child_count = node_ref.children.len();
    for (i, &child) in node_ref.children.iter().enumerate() {
        let is_last_child = i == child_count - 1;
        let new_prefix = if is_root {
            String::new()
        } else {
            format!("{}{}    ", prefix, if is_last { " " } else { "│" })
        };
        format_node_pretty(
            f,
            child,
            cursor,
            &new_prefix,
            false,
            is_last_child,
        )?;
    }

    Ok(())
}

unsafe fn update_spent(node: *mut CallTreeNode) {
    let node = &mut *node;
    node.children.iter_mut().for_each(|&mut child| unsafe {
        // It should be impossible for this to underflow since the amount spent
        // in all child nodes is always less than or equal to the amount spent
        // in the parent node.
        node.elem.spent -= (*child).elem.spent;
        update_spent(child);
    });
}

unsafe fn free_tree(root: *mut CallTreeNode) {
    let mut node = Box::from_raw(root);

    let mut children = Vec::new();
    mem::swap(&mut node.children, &mut children);

    for child in children {
        free_tree(child);
    }
}

struct CallTreeNode {
    elem: CallTreeElem,
    children: Vec<*mut Self>,
    parent: Option<*mut Self>,
}

impl CallTreeNode {
    fn new(elem: CallTreeElem) -> *mut Self {
        Box::leak(Box::new(Self {
            elem,
            children: Vec::new(),
            parent: None,
        }))
    }

    fn with_parent(elem: CallTreeElem, parent: *mut Self) -> *mut Self {
        Box::leak(Box::new(Self {
            elem,
            children: Vec::new(),
            parent: Some(parent),
        }))
    }
}

/// An iterator over a [`CallTree`].
///
/// It starts at the rightmost node and proceeds leftward towards its siblings,
/// up toward the root.
struct CallTreeIter<'a> {
    tree: Option<Subtree>,
    _marker: PhantomData<&'a ()>,
}

impl<'a> Iterator for CallTreeIter<'a> {
    type Item = &'a CallTreeElem;

    fn next(&mut self) -> Option<Self::Item> {
        // SAFETY: This is safe since we guarantee that the tree exists between
        // the root and the current node. This is done by ensuring the iterator
        // can only exist during the lifetime of the tree.
        unsafe {
            let tree = self.tree.as_mut()?;

            let node = tree.node;
            let elem = &(*node).elem;

            if node == tree.root {
                self.tree = None;
                return Some(elem);
            }

            let parent = (*node).parent.expect(
                "There should always be a parent in a subtree before the root",
            );

            tree.node = {
                let node_index = (*parent)
                    .children
                    .iter()
                    .position(|&n| n == node)
                    .expect("The child must be the its parent's child");

                if node_index == 0 {
                    parent
                } else {
                    let sibling_index = node_index - 1;
                    let mut next_node = (*parent).children[sibling_index];

                    while !(*next_node).children.is_empty() {
                        let last_index = (*next_node).children.len() - 1;
                        next_node = (*next_node).children[last_index];
                    }

                    next_node
                }
            };

            Some(elem)
        }
    }
}
