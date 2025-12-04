// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Cursor-based n-ary tree for tracking hierarchical contract calls.
//!
//! This module implements a specialized tree structure for the Piecrust VM that
//! maintains a "current position" cursor moving through the tree as contracts
//! call each other and return.
//!
//! ## Structure
//!
//! - **N-ary tree**: Each node can have any number of children
//! - **Bidirectional pointers**: Parent and child pointers enable both upward
//!   and downward traversal
//! - **Cursor semantics**: A current position pointer tracks the active
//!   contract call
//! - **Manual memory management**: Uses `Box::leak()` for allocation and
//!   recursive deallocation
//!
//! ## Operations
//!
//! - **`push()`**: Add child to current node and move cursor down (making a
//!   call)
//! - **`move_up()`**: Move cursor to parent with gas recording (returning from
//!   a call)
//! - **`move_up_prune()`**: Move up while removing current subtree (reverting a
//!   call)
//! - **`update_spent()`**: Recursively adjust gas accounting across the tree
//! - **`iter()`**: Traverse from current position through all descendants in
//!   reverse post-order
//!
//! ## Iterator Behavior
//!
//! The iterator does **not** traverse the entire tree. It only visits nodes
//! from the current cursor position downward through all its descendants.
//!
//! Traversal order is reverse post-order:
//!
//! 1. Rightmost leaf nodes are visited first i.e., recursively go to the
//!    rightmost leaf in right subtrees
//! 2. Then traverse left siblings (that become rightmost) and, if existent,
//!    their subtrees again, with rightmost leaf priority again
//! 3. Visit current node (current cursor) last and stop
//!
//! This mirrors how contract calls unwind their deepest
//! calls complete first. More information is in the `iter()` documentation.
//!
//! ## Safety
//!
//! Internally uses raw pointers with manual memory management. Nodes are
//! allocated via `Box::leak()` and freed via `free_tree()`. Memory is
//! automatically cleaned up when `CallTree` is dropped.

use std::fmt;
use std::marker::PhantomData;
use std::mem;

use piecrust_uplink::ContractId;

/// An element of the call tree, representing a single contract call.
///
/// Each element tracks the contract being called along with its resource usage:
/// - Gas limit and spending for the call
/// - Memory length allocated during execution
#[derive(Debug, Clone, Copy)]
pub struct CallTreeElem {
    /// The unique identifier of the contract being called
    pub contract_id: ContractId,
    /// The gas limit available for this contract call
    pub limit: u64,
    /// The amount of gas spent by this contract call (including child calls)
    pub spent: u64,
    /// The length of memory allocated for this contract call
    pub mem_len: usize,
}

/// A cursor-based tree structure for tracking hierarchical contract calls.
///
/// The tree maintains a "current position" pointer that moves through the tree
/// as contracts call each other and return. The internal pointer represents:
/// - `None`: Empty tree (no calls have been made)
/// - `Some(node)`: Points to the currently executing contract call
///
/// # Memory Management
///
/// Uses raw pointers with manual allocation (`Box::leak`) and deallocation
/// (`free_tree`). The `Drop` implementation ensures proper cleanup.
#[derive(Default)]
pub struct CallTree(Option<*mut CallTreeNode>);

impl fmt::Debug for CallTree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        /// Helper function to format a node and its children in a tree
        /// structure. Shows the ContractId's as calls in a hierarchical manner.
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

            // Format contract ID (first 4 bytes only)
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

        // Use pretty printing for {:#?}, default pointer format for {:?}
        if f.alternate() {
            match self.0 {
                None => write!(f, "[]"),
                Some(root) => {
                    // Find the actual root of the tree
                    let mut actual_root = root;
                    while let Some(parent) = unsafe { (*actual_root).parent } {
                        actual_root = parent;
                    }

                    unsafe {
                        format_node_pretty(f, actual_root, root, "", true, true)
                    }
                }
            }
        } else {
            f.debug_tuple("CallTree").field(&self.0).finish()
        }
    }
}

impl CallTree {
    /// Creates a new empty call tree, starting with the given contract.
    pub(crate) const fn new() -> Self {
        Self(None)
    }

    /// Pushes a new contract call as a child of the current node and moves to
    /// it.
    ///
    /// This represents a contract making a call to another contract. The tree
    /// grows downward, adding the new call as the last child of the current
    /// node.
    ///
    /// # Tree Structure Impact
    ///
    /// - If tree is empty: Creates root node and positions cursor there
    /// - If tree is not empty: Adds new node as child of current, moves cursor
    ///   to child
    ///
    /// The new node becomes the current position, allowing subsequent `push()`
    /// calls to create deeper call chains.
    pub(crate) fn push(&mut self, elem: CallTreeElem) {
        match self.0 {
            // Tree is empty: create root node
            None => self.0 = Some(CallTreeNode::new(elem)),
            // Tree exists: add as child of current node
            Some(inner) => unsafe {
                // Create new node with current as parent
                let node = CallTreeNode::with_parent(elem, inner);
                // Add new node to current's children
                (*inner).children.push(node);
                // Move cursor down to new node
                self.0 = Some(node)
            },
        }
    }

    /// Returns from the current contract call to its parent, recording gas
    /// spent.
    ///
    /// This represents a contract call completing and returning control to its
    /// caller. The spent gas is recorded on the current node before moving
    /// up.
    ///
    /// # Memory Management
    ///
    /// If moving up from the root (no parent), the entire tree is freed since
    /// the call chain has completed.
    ///
    /// # Returns
    ///
    /// The element of the node we're moving up from (with updated spent value),
    /// or `None` if the tree is empty.
    pub(crate) fn move_up(&mut self, spent: u64) -> Option<CallTreeElem> {
        // inner = current node we're pointing to = our cursor
        self.0.map(|inner| unsafe {
            // Record gas spent at current node
            (*inner).elem.spent = spent;
            let elem = (*inner).elem;

            // Get parent pointer from current node
            let parent = (*inner).parent;
            // If at root, deallocate entire tree
            if parent.is_none() {
                free_tree(inner);
            }
            // Move cursor up to parent
            self.0 = parent;

            elem
        })
    }

    /// Returns to parent while pruning the current node and its entire subtree.
    ///
    /// This represents a contract call that failed or was reverted. The current
    /// node and all its descendants are removed from the tree and freed.
    ///
    /// # Use Case
    ///
    /// When a contract call reverts, we want to undo not just that call, but
    /// all calls it made (its children). This operation removes the entire
    /// subtree.
    ///
    /// # Parent Adjustment
    ///
    /// The parent's children vector has the current node removed via `pop()`,
    /// assuming it's the last child (which it is, since we always push to the
    /// end).
    ///
    /// # Returns
    ///
    /// The element being pruned, or `None` if the tree is empty.
    pub(crate) fn move_up_prune(&mut self) -> Option<CallTreeElem> {
        // inner = current node we're pointing to = our cursor
        self.0.map(|inner| unsafe {
            let elem = (*inner).elem;

            // Get parent pointer before freeing
            let parent = (*inner).parent;
            // Remove current node from parent's children
            if let Some(parent) = parent {
                (*parent).children.pop();
            }
            // Deallocate current node and entire subtree
            free_tree(inner);
            // Move cursor up to parent
            self.0 = parent;

            elem
        })
    }

    /// Updates gas spending to reflect the total spent by current node.
    ///
    /// This performs recursive gas accounting to separate the direct gas spent
    /// by each contract from the gas spent by its child calls.
    ///
    /// # Gas Accounting Logic
    ///
    /// The `spent` parameter represents the **total** gas spent by this
    /// contract, including all child calls. The recursive `update_spent()`
    /// function subtracts each child's spending from the parent, leaving
    /// only the parent's direct spend.
    ///
    /// Example: If parent spent 1000 total, child A spent 300, child B spent
    /// 200, then parent's direct spend is 500 (1000 - 300 - 200).
    pub(crate) fn update_spent(&mut self, spent: u64) {
        if let Some(inner) = self.0 {
            unsafe {
                (*inner).elem.spent = spent;
                update_spent(inner);
            }
        }
    }

    /// Returns the ancestor at distance `n` from the current position.
    ///
    /// This traverses upward through parent pointers to find ancestors.
    ///
    /// # Parameters
    ///
    /// - `n = 0`: Returns current node
    /// - `n = 1`: Returns parent of current node
    /// - `n = 2`: Returns grandparent, etc.
    ///
    /// # Returns
    ///
    /// `Some(elem)` if an ancestor exists at distance `n`, `None` if we reach
    /// the root before counting to `n` (or if tree is empty).
    pub(crate) fn nth_parent(&self, n: usize) -> Option<CallTreeElem> {
        let mut current = self.0;

        let mut i = 0;
        while i < n {
            current = current.and_then(|inner| unsafe { (*inner).parent });
            i += 1;
        }

        current.map(|inner| unsafe { (*inner).elem })
    }

    /// Returns the call stack path from current position to root.
    ///
    /// Traverses parent pointers from the current node up to the root,
    /// collecting contract IDs along the way.
    ///
    /// # Returns
    ///
    /// A vector of contract IDs representing the call chain:
    /// - `[0]`: Current contract
    /// - `[1]`: Parent (caller of current)
    /// - `[2]`: Grandparent, etc.
    /// - `[n-1]`: Root (first contract called)
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
    ///
    /// Traverses upward to find the root node, then recursively frees the
    /// entire tree starting from the root. This ensures all nodes are
    /// properly deallocated regardless of the current cursor position.
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

    /// Returns an iterator over the current node and its descendants.
    ///
    /// Traverses in **reverse post-order** (rightmost leaf first):
    /// deepest-rightmost children first, then left siblings, then parents.
    /// This matches how contract calls unwind (deepest calls complete first).
    ///
    /// # Example
    ///
    /// **Notation**:
    /// - `->` means "calls"
    /// - `[]` groups sibling calls (sequential calls made by the same contract)
    /// - `,` separates siblings (calls made one after another)
    ///
    /// For tree `A -> B -> [D, E]` and `A -> C`, if positioned at A:
    /// A calls B, which calls D then E, then A calls C.
    ///
    /// Tree structure:
    /// ```text
    ///      A
    ///     / \
    ///    B   C
    ///   / \
    ///  D   E
    /// ```
    ///
    /// Iterator yields: C, E, D, B, A (rightmost leaves first)
    ///
    /// For tree where TC calls A, which makes two sequential calls to TC and D:
    /// - TC means "Transfer contract"
    /// - A means "Alice contract"
    /// - B means "Bob contract"
    /// - C means "Charlie contract"
    /// - D means "David contract"
    ///
    /// Tree structure:
    /// ```text
    /// TC (root)
    ///     |
    ///     A
    ///    / \
    ///  TC   D
    ///  /
    /// C
    /// ```
    ///
    /// Call sequence: `TC -> [A -> [TC -> C], D]`
    /// - TC calls A
    /// - A calls TC (which calls C), then A calls D
    /// - TC and D are siblings (both called by A sequentially)
    ///
    /// Iterator at root TC yields: D, C, TC (from A), A, TC (root)
    /// Iterator at A yields: D, C, TC, A
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

/// Represents the iteration state over a subtree.
///
/// Tracks both the root of the subtree (for boundary checking) and the
/// current node being iterated.
struct Subtree {
    /// The root of the subtree being iterated (boundary limit)
    root: *mut CallTreeNode,
    /// The current node in the iteration
    node: *mut CallTreeNode,
}

impl Drop for CallTree {
    fn drop(&mut self) {
        self.clear()
    }
}

/// Recursively adjusts gas spending to separate direct spend from child spend.
///
/// For each node, subtracts the spending of all direct children from the node's
/// total spending, leaving only the gas spent directly by that contract.
///
/// # Safety
///
/// Assumes `node` is a valid pointer to a CallTreeNode. The caller must ensure
/// the pointer remains valid for the duration of this call.
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

/// Recursively deallocates a tree node and all its descendants.
///
/// Uses post-order traversal to free children before freeing the parent,
/// ensuring no dangling pointers.
///
/// # Safety
///
/// - `root` must be a valid pointer obtained from `Box::leak()`
/// - `root` and all descendants must not be accessed after this call
/// - Each node should only be freed once
unsafe fn free_tree(root: *mut CallTreeNode) {
    let mut node = Box::from_raw(root);

    let mut children = Vec::new();
    mem::swap(&mut node.children, &mut children);

    for child in children {
        free_tree(child);
    }
}

/// Internal tree node structure.
///
/// Each node contains:
/// - The contract call data (`elem`) i.e., element at this node
/// - Pointers to child nodes (enabling n-ary tree structure)
/// - Optional parent pointer (enabling upward traversal)
///
/// Nodes are heap-allocated via `Box::leak()` and must be manually freed.
struct CallTreeNode {
    /// The contract call data stored in this node
    elem: CallTreeElem,
    /// Child nodes representing calls made by this contract
    children: Vec<*mut Self>,
    /// Pointer to parent node (None for root)
    parent: Option<*mut Self>,
}

impl CallTreeNode {
    /// Creates a new root node (no parent).
    ///
    /// Allocates the node on the heap via `Box::leak()`, returning a raw
    /// pointer that must be manually freed later.
    fn new(elem: CallTreeElem) -> *mut Self {
        Box::leak(Box::new(Self {
            elem,
            children: Vec::new(),
            parent: None,
        }))
    }

    /// Creates a new child node with a parent pointer.
    ///
    /// Allocates the node on the heap via `Box::leak()`, returning a raw
    /// pointer that must be manually freed later.
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

    /// Advances the iterator to the next node in reverse post-order.
    ///
    /// Yields the current node, then moves to the next node: either the
    /// rightmost leaf of the left sibling's subtree, or the parent.
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

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function to create a contract ID for testing
    fn contract_id(n: u8) -> ContractId {
        let mut bytes = [0u8; 32];
        bytes[0] = n;
        ContractId::from_bytes(bytes)
    }

    // Helper function to create a test element
    fn elem(id: u8, limit: u64, spent: u64, mem_len: usize) -> CallTreeElem {
        CallTreeElem {
            contract_id: contract_id(id),
            limit,
            spent,
            mem_len,
        }
    }

    #[test]
    fn test_basic_operations() {
        // Test empty tree
        let mut tree = CallTree::new();
        assert!(tree.nth_parent(0).is_none());
        assert!(tree.call_ids().is_empty());
        assert_eq!(tree.iter().count(), 0);

        // Test single element - verify all fields preserved
        tree.push(elem(1, 999999, 0, 8192));
        let current = tree.nth_parent(0).unwrap();
        assert_eq!(current.contract_id, contract_id(1));
        assert_eq!(current.limit, 999999);
        assert_eq!(current.spent, 0);
        assert_eq!(current.mem_len, 8192);

        // Test move_up updates spent
        let returned = tree.move_up(500).unwrap();
        assert_eq!(returned.spent, 500);
        assert!(tree.nth_parent(0).is_none());
    }

    #[test]
    fn test_linear_chain_navigation() {
        let mut tree = CallTree::new();

        // Build 5-element chain and test nth_parent
        for i in 1u8..=5 {
            let limit = 1000u64 - (i as u64) * 100;
            tree.push(elem(i, limit, 0, i as usize * 100));
        }

        // Verify nth_parent navigation
        assert_eq!(tree.nth_parent(0).unwrap().contract_id, contract_id(5));
        assert_eq!(tree.nth_parent(1).unwrap().contract_id, contract_id(4));
        assert_eq!(tree.nth_parent(4).unwrap().contract_id, contract_id(1));
        assert!(tree.nth_parent(5).is_none());

        // Verify call_ids
        let ids = tree.call_ids();
        assert_eq!(ids.len(), 5);
        for i in 0..5 {
            assert_eq!(*ids[i], contract_id((5 - i) as u8));
        }

        // Test sequential move_ups
        for i in (1..=5).rev() {
            let e = tree.move_up(i * 100).unwrap();
            assert_eq!(e.contract_id, contract_id(i as u8));
            assert_eq!(e.spent, i * 100);
        }
        assert!(tree.nth_parent(0).is_none());
    }

    #[test]
    fn test_iterator_behavior() {
        let mut tree = CallTree::new();
        tree.push(elem(1, 1000, 0, 100));
        tree.push(elem(2, 900, 0, 200));
        tree.push(elem(3, 800, 0, 300));

        // At leaf, iterator shows only current
        assert_eq!(tree.iter().count(), 1);

        // Move up, iterator shows subtree
        tree.move_up(30);
        let elements: Vec<_> = tree.iter().collect();
        assert_eq!(elements.len(), 2);
        assert_eq!(elements[0].contract_id, contract_id(3));
        assert_eq!(elements[1].contract_id, contract_id(2));
    }

    #[test]
    fn test_iterator_advanced() {
        let mut tree = CallTree::new();

        // Empty tree iterator
        assert_eq!(tree.iter().count(), 0);

        // Complex tree structure: A with children B (with D) and C
        tree.push(elem(1, 1000, 0, 100)); // A
        tree.push(elem(2, 900, 0, 200)); // B
        tree.push(elem(4, 800, 0, 400)); // D
        tree.move_up(40);
        tree.move_up(20);
        tree.push(elem(3, 700, 0, 300)); // C
        tree.move_up(30);

        // Iterator at A: rightmost leaf to root (C, D, B, A)
        let elements: Vec<_> = tree.iter().collect();
        assert_eq!(elements.len(), 4);
        assert_eq!(elements[0].contract_id, contract_id(3));
        assert_eq!(elements[3].contract_id, contract_id(1));

        // Multiple iterators work independently
        let iter1 = tree.iter();
        let iter2 = tree.iter();
        assert_eq!(iter1.count(), iter2.count());

        // Iterator empty after clear
        tree.clear();
        assert_eq!(tree.iter().count(), 0);
    }

    #[test]
    fn test_tree_with_multiple_siblings() {
        let mut tree = CallTree::new();

        // Build tree structure:
        //      A
        //     / \
        //    B   C
        //   / \
        //  D   E

        // Build: A -> B -> D
        tree.push(elem(1, 1000, 0, 100)); // A
        tree.push(elem(2, 900, 0, 200)); // B
        tree.push(elem(4, 800, 0, 400)); // D
        tree.move_up(100); // Back to B

        // Build: B -> E
        tree.push(elem(5, 700, 0, 500)); // E
        tree.move_up(150); // Back to B
        tree.move_up(250); // Back to A

        // Build: A -> C
        tree.push(elem(3, 600, 0, 300)); // C

        // Verify structure at C
        assert_eq!(tree.nth_parent(0).unwrap().contract_id, contract_id(3)); // C
        assert_eq!(tree.nth_parent(1).unwrap().contract_id, contract_id(1)); // A

        // Iterator at C should yield just C (no children)
        let elements: Vec<_> = tree.iter().collect();
        assert_eq!(elements.len(), 1);
        assert_eq!(elements[0].contract_id, contract_id(3));

        // Move back to A and check iterator
        tree.move_up(200);
        let elements: Vec<_> = tree.iter().collect();
        // Should iterate over entire tree rooted at A
        // Order: rightmost leaf to root (C, E, D, B, A)
        assert_eq!(elements.len(), 5);
        assert_eq!(elements[0].contract_id, contract_id(3)); // C (rightmost)
        assert_eq!(elements[1].contract_id, contract_id(5)); // E
        assert_eq!(elements[2].contract_id, contract_id(4)); // D
        assert_eq!(elements[3].contract_id, contract_id(2)); // B
        assert_eq!(elements[4].contract_id, contract_id(1)); // A

        // Test flat siblings (Root -> A, B, C)
        tree.clear(); // Reset tree
        assert!(tree.nth_parent(0).is_none()); // Ensure empty tree

        tree.push(elem(10, 1000, 0, 100)); // Root
        tree.push(elem(11, 900, 0, 200)); // A
        tree.move_up(20); // Back to Root
        tree.push(elem(12, 800, 0, 300)); // B
        tree.move_up(30); // Back to Root
        tree.push(elem(13, 700, 0, 400)); // C

        assert_eq!(tree.nth_parent(0).unwrap().contract_id, contract_id(13));
        tree.move_up(40);

        // Iterator should show all: C, B, A, Root
        let elements: Vec<_> = tree.iter().collect();
        assert_eq!(elements.len(), 4);
        assert_eq!(elements[0].contract_id, contract_id(13)); // C
        assert_eq!(elements[1].contract_id, contract_id(12)); // B
        assert_eq!(elements[2].contract_id, contract_id(11)); // A
        assert_eq!(elements[3].contract_id, contract_id(10)); // Root
    }

    #[test]
    fn test_complex_nested_tree() {
        let mut tree = CallTree::new();

        // Build tree:
        //        Root(1)
        //       /  |  \
        //      A   B   C
        //     (2) (3) (4)
        //     /|   |
        //    D E   F
        //   (5)(6)(7)
        //   /
        //  G(8)

        // Root -> A -> D -> G
        tree.push(elem(1, 10000, 0, 100));
        tree.push(elem(2, 9000, 0, 200));
        tree.push(elem(5, 8000, 0, 500));
        tree.push(elem(8, 7000, 0, 800));
        tree.move_up(80); // Back to D
        tree.move_up(50); // Back to A

        // A -> E
        tree.push(elem(6, 6000, 0, 600));
        tree.move_up(60); // Back to A
        tree.move_up(20); // Back to Root

        // Root -> B -> F
        tree.push(elem(3, 5000, 0, 300));
        tree.push(elem(7, 4000, 0, 700));
        tree.move_up(70); // Back to B
        tree.move_up(30); // Back to Root

        // Root -> C
        tree.push(elem(4, 3000, 0, 400));

        // Verify current position
        assert_eq!(tree.nth_parent(0).unwrap().contract_id, contract_id(4)); // C
        assert_eq!(tree.nth_parent(1).unwrap().contract_id, contract_id(1)); // Root

        // call_ids should show path from current to root
        let ids = tree.call_ids();
        assert_eq!(ids.len(), 2);
        assert_eq!(*ids[0], contract_id(4)); // C
        assert_eq!(*ids[1], contract_id(1)); // Root
    }

    #[test]
    fn test_pruning_operations() {
        let mut tree = CallTree::new();

        // Test 1: Prune leaf node
        tree.push(elem(1, 1000, 0, 100));
        tree.push(elem(2, 900, 0, 200));
        tree.push(elem(3, 800, 0, 300));

        let pruned = tree.move_up_prune().unwrap();
        assert_eq!(pruned.contract_id, contract_id(3));
        assert_eq!(tree.nth_parent(0).unwrap().contract_id, contract_id(2));
        assert_eq!(tree.iter().count(), 1); // Just A

        // Test 2: Prune branch with children
        tree.move_up(20);
        tree.push(elem(4, 700, 0, 400)); // New child
        tree.push(elem(5, 600, 0, 500)); // Grandchild
        tree.move_up(50);
        tree.move_up(40); // Back to root

        // Prune node 4 (has child 5)
        tree.push(elem(6, 500, 0, 600)); // Another child
        tree.move_up(60);
        tree.push(elem(7, 400, 0, 700)); // Attach to root
        tree.push(elem(8, 300, 0, 800)); // Child of 7
        tree.move_up(80);

        let pruned = tree.move_up_prune().unwrap();
        assert_eq!(pruned.contract_id, contract_id(7)); // Prunes entire subtree
        assert_eq!(tree.nth_parent(0).unwrap().contract_id, contract_id(1));

        // Test 3: Prune root node - tree becomes empty
        tree.clear();
        tree.push(elem(10, 1000, 0, 100));
        let pruned = tree.move_up_prune().unwrap();
        assert_eq!(pruned.contract_id, contract_id(10));
        assert!(tree.nth_parent(0).is_none());
        assert_eq!(tree.iter().count(), 0);
    }

    #[test]
    fn test_prune_with_siblings() {
        let mut tree = CallTree::new();

        // Build: Root with three children A, B, C
        tree.push(elem(1, 1000, 0, 100)); // Root
        tree.push(elem(2, 900, 0, 200)); // A with child
        tree.push(elem(3, 850, 0, 250)); // A's child
        tree.move_up_prune(); // Prune A's child
        tree.move_up(20); // Back to Root

        tree.push(elem(4, 800, 0, 300)); // B with child
        tree.push(elem(5, 750, 0, 350)); // B's child
        tree.move_up_prune(); // Prune B's child
        tree.move_up(40); // Back to Root

        tree.push(elem(6, 700, 0, 400)); // C
        tree.move_up_prune(); // Prune C

        // Should be at Root with only A, B remaining
        assert_eq!(tree.nth_parent(0).unwrap().contract_id, contract_id(1));
        let elements: Vec<_> = tree.iter().collect();
        assert_eq!(elements.len(), 3); // B, A, Root
        assert_eq!(elements[0].contract_id, contract_id(4)); // B
        assert_eq!(elements[1].contract_id, contract_id(2)); // A
        assert_eq!(elements[2].contract_id, contract_id(1)); // Root
    }

    #[test]
    fn test_gas_accounting() {
        let mut tree = CallTree::new();

        // Simple: parent with one child
        tree.push(elem(1, 1000, 0, 100));
        tree.push(elem(2, 500, 0, 200));
        tree.move_up(300);
        tree.update_spent(700);
        assert_eq!(tree.nth_parent(0).unwrap().spent, 400); // 700 - 300

        // Siblings: root with two sequential children
        tree.clear();
        tree.push(elem(1, 10000, 0, 100));
        tree.push(elem(2, 9000, 0, 200));
        tree.move_up(300);
        tree.push(elem(3, 8000, 0, 300));
        tree.move_up(200);
        tree.update_spent(1000);
        assert_eq!(tree.nth_parent(0).unwrap().spent, 500); // 1000 - 300 - 200

        // Deep linear chain: 5 levels with recursive subtraction
        tree.clear();
        tree.push(elem(1, 10000, 0, 100));
        tree.push(elem(2, 9000, 0, 200));
        tree.push(elem(3, 8000, 0, 300));
        tree.push(elem(4, 7000, 0, 400));
        tree.push(elem(5, 6000, 0, 500));
        tree.move_up(100);
        tree.move_up(200);
        tree.move_up(300);
        tree.move_up(400);
        tree.update_spent(1500);
        assert_eq!(tree.nth_parent(0).unwrap().spent, 1100); // 1500 - 400 (direct child only)
        assert_eq!(tree.nth_parent(0).unwrap().limit, 10000); // Limits preserved

        // Zero gas edge case
        tree.clear();
        tree.push(elem(4, 1000, 0, 100));
        tree.push(elem(5, 900, 0, 200));
        tree.move_up(0);
        tree.update_spent(0);
        assert_eq!(tree.nth_parent(0).unwrap().spent, 0);
    }

    #[test]
    fn test_edge_cases() {
        let mut tree = CallTree::new();

        // Operations on empty tree
        tree.clear(); // Should not panic
        assert!(tree.move_up(100).is_none());
        assert!(tree.move_up_prune().is_none());
        tree.update_spent(1000); // Should not panic
        assert!(tree.nth_parent(0).is_none());
        assert!(tree.nth_parent(100).is_none());
        assert!(tree.call_ids().is_empty());

        // Build tree and test edge cases
        tree.push(elem(1, 1000, 0, 100));
        tree.push(elem(2, 900, 0, 200));
        tree.push(elem(3, 800, 0, 300));

        // nth_parent with large N
        assert!(tree.nth_parent(100).is_none());

        // Move up beyond root
        tree.move_up(30);
        tree.move_up(20);
        tree.move_up(10);
        assert!(tree.nth_parent(0).is_none());
        assert!(tree.move_up(100).is_none()); // Already empty

        // Push on cleared tree
        tree.push(elem(4, 700, 0, 400));
        assert_eq!(tree.nth_parent(0).unwrap().contract_id, contract_id(4));

        // Consecutive clears
        tree.clear();
        tree.clear();
        tree.clear();
        assert!(tree.nth_parent(0).is_none());

        // Extreme gas values
        tree.push(elem(1, u64::MAX, 0, 100));
        tree.push(elem(2, u64::MAX - 1, 0, 200));
        tree.move_up(u64::MAX - 1000);
        tree.update_spent(u64::MAX);
        assert!(tree.nth_parent(0).unwrap().spent <= u64::MAX);
    }

    #[test]
    fn test_tree_clearing() {
        let mut tree = CallTree::new();

        // Build complex tree
        tree.push(elem(1, 10000, 0, 100));
        for i in 2..=10 {
            tree.push(elem(i, 9000, 0, i as usize * 100));
        }

        // Clear and verify
        tree.clear();
        assert!(tree.nth_parent(0).is_none());
        assert_eq!(tree.iter().count(), 0);

        // Clear after pruning
        tree.push(elem(1, 1000, 0, 100));
        tree.push(elem(2, 900, 0, 200));
        tree.move_up_prune();
        tree.clear();
        assert!(tree.nth_parent(0).is_none());
    }

    #[test]
    fn test_memory_safety() {
        let mut tree = CallTree::new();

        // Parent-child pointer consistency
        tree.push(elem(1, 1000, 0, 100));
        tree.push(elem(2, 900, 0, 200));
        tree.move_up(20);
        tree.push(elem(3, 800, 0, 300));

        let c = tree.nth_parent(0).unwrap();
        assert_eq!(c.contract_id, contract_id(3));
        let root = tree.nth_parent(1).unwrap();
        assert_eq!(root.contract_id, contract_id(1));
        assert_eq!(tree.call_ids().len(), 2);

        // ContractId references remain valid
        let ids = tree.call_ids();
        let id_copy = *ids[0];
        assert_eq!(id_copy, contract_id(3));

        // mem_len preservation
        tree.clear();
        let mem_lens = vec![1024, 2048, 4096, 8192];
        for (i, &mem_len) in mem_lens.iter().enumerate() {
            tree.push(elem((i + 1) as u8, 1000, 0, mem_len));
        }
        for (i, &expected) in mem_lens.iter().enumerate() {
            assert_eq!(
                tree.nth_parent(mem_lens.len() - 1 - i).unwrap().mem_len,
                expected
            );
        }
    }

    #[test]
    fn test_revert_and_stress() {
        let mut tree = CallTree::new();

        // Revert simulation pattern
        tree.push(elem(1, 10000, 0, 1000));
        tree.push(elem(2, 9000, 0, 2000));
        tree.push(elem(3, 8000, 0, 3000));
        tree.push(elem(4, 7000, 0, 4000));
        tree.move_up(400);
        tree.move_up(300);

        let mem_lens: Vec<_> = tree.iter().map(|e| e.mem_len).collect();
        assert!(mem_lens.contains(&3000) && mem_lens.contains(&2000));

        // Stress test with large tree
        tree.clear();
        for i in 1..=100 {
            tree.push(elem(i as u8, 10000 - i * 10, 0, i as usize * 10));
        }
        for n in 0..100 {
            assert!(tree.nth_parent(n).is_some());
        }
        assert!(tree.nth_parent(100).is_none());

        // Interleaved operations
        tree.clear();
        tree.push(elem(1, 1000, 0, 100));
        tree.push(elem(2, 900, 0, 200));
        let _ = tree.iter().count();
        tree.push(elem(3, 800, 0, 300));
        assert_eq!(tree.call_ids().len(), 3);
        tree.move_up(30);
        tree.push(elem(4, 700, 0, 400));
        tree.move_up_prune();
        tree.update_spent(500);
        tree.clear();
    }

    #[test]
    fn test_drop_behavior() {
        // Verify Drop doesn't crash
        {
            let mut tree = CallTree::new();
            tree.push(elem(1, 1000, 0, 100));
            tree.push(elem(2, 900, 0, 200));
            tree.push(elem(3, 800, 0, 300));
        } // Drop called here
        assert!(true);
    }

    #[test]
    fn test_contract_call_patterns() {
        let mut tree = CallTree::new();

        // Nested calls: A -> B -> C
        tree.push(elem(1, 10000, 0, 1000));
        tree.push(elem(2, 8000, 0, 2000));
        tree.push(elem(3, 6000, 0, 3000));

        // Unwind with gas accounting
        let c = tree.move_up(1500).unwrap();
        assert_eq!(c.spent, 1500);
        let b = tree.move_up(3000).unwrap();
        assert_eq!(b.spent, 3000);
        let a = tree.move_up(5000).unwrap();
        assert_eq!(a.spent, 5000);
        assert!(tree.nth_parent(0).is_none());

        // Failed call with prune
        tree.push(elem(1, 10000, 0, 1000));
        tree.push(elem(2, 5000, 0, 2000));
        tree.move_up_prune(); // B fails
        assert_eq!(tree.nth_parent(0).unwrap().contract_id, contract_id(1));
        tree.move_up(1000);

        // Sequential sibling calls
        tree.push(elem(1, 10000, 0, 1000));
        tree.push(elem(2, 3000, 0, 2000));
        tree.move_up(500);
        tree.push(elem(3, 3000, 0, 3000));
        tree.move_up(600);
        tree.push(elem(4, 3000, 0, 4000));
        tree.move_up(700);
        assert_eq!(tree.iter().count(), 4); // All children visible

        // Pattern from documentation: TC -> [A -> [TC -> C, D]]
        // Tree structure:
        //     TC (root)
        //        |
        //        A
        //       / \
        //     TC   D
        //     /
        //    C
        tree.clear();

        // Build: TC (root)
        tree.push(elem(10, 10000, 0, 1000)); // TC (Transfer Contract)

        // Build: TC -> A
        tree.push(elem(20, 9000, 0, 2000)); // A (Alice)

        // Build: A -> TC (nested call back to TC)
        tree.push(elem(10, 8000, 0, 1000)); // TC again (same contract ID)

        // Build: TC -> C
        tree.push(elem(30, 7000, 0, 3000)); // C (Charlie)
        tree.move_up(300); // Back to nested TC
        tree.move_up(100); // Back to A

        // Build: A -> D (sibling to the nested TC call)
        tree.push(elem(40, 6000, 0, 4000)); // D (David)
        tree.move_up(400); // Back to A
        tree.move_up(200); // Back to root TC

        // Verify tree structure at root TC
        assert_eq!(tree.nth_parent(0).unwrap().contract_id, contract_id(10)); // TC root

        // Iterator at root TC should yield: D, C, TC (from A), A, TC (root)
        let elements: Vec<_> = tree.iter().collect();
        assert_eq!(elements.len(), 5);
        assert_eq!(elements[0].contract_id, contract_id(40)); // D (rightmost leaf)
        assert_eq!(elements[1].contract_id, contract_id(30)); // C
        assert_eq!(elements[2].contract_id, contract_id(10)); // TC (nested, from A)
        assert_eq!(elements[3].contract_id, contract_id(20)); // A
        assert_eq!(elements[4].contract_id, contract_id(10)); // TC (root)
    }

    #[test]
    fn test_complex_call_scenarios() {
        let mut tree = CallTree::new();

        // Deep call chain
        for i in 1u8..=5 {
            let limit = 10000u64 - (i as u64) * 1000;
            tree.push(elem(i, limit, 0, i as usize * 1000));
        }
        assert_eq!(tree.call_ids().len(), 5);
        for i in (1..=5).rev() {
            let e = tree.move_up(i * 100).unwrap();
            assert_eq!(e.contract_id, contract_id(i as u8));
        }

        // Mixed success/failure
        tree.push(elem(1, 10000, 0, 1000));
        tree.push(elem(2, 3000, 0, 2000));
        tree.move_up(500); // Success
        tree.push(elem(3, 3000, 0, 3000));
        tree.move_up_prune(); // Failure
        tree.push(elem(4, 3000, 0, 4000));
        tree.move_up(700); // Success

        let elements: Vec<_> = tree.iter().collect();
        let ids: Vec<_> = elements
            .iter()
            .map(|e| e.contract_id.to_bytes()[0])
            .collect();
        assert!(ids.contains(&4) && ids.contains(&2) && !ids.contains(&3));
    }
}
