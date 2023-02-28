// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::collections::btree_map::{BTreeMap, Entry};

use piecrust_uplink::ModuleId;

use crate::instance::WrappedInstance;

#[derive(Debug, Default)]
pub struct CallStack {
    // map of all instances together with a count in the stack.
    instance_map: BTreeMap<ModuleId, (*mut WrappedInstance, u64)>,
    stack: Vec<StackElement>,
}

impl CallStack {
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a new instance to the call stack.
    ///
    /// # Panics
    /// If an instance of the same module ID is already in the stack, a panic
    /// will occur.
    pub fn push_instance(
        &mut self,
        module_id: ModuleId,
        limit: u64,
        instance: WrappedInstance,
    ) {
        if self.instance_map.get(&module_id).is_some() {
            panic!("Module already in the stack: {module_id:?}",);
        }

        let instance = Box::new(instance);
        let instance = Box::leak(instance) as *mut WrappedInstance;

        self.instance_map.insert(module_id, (instance, 1));
        self.stack.push(StackElement { module_id, limit });
    }

    /// Push an element to the call stack.
    ///
    /// # Panics
    /// If an instance of the given module ID is absent from the stack.
    pub fn push(&mut self, module_id: ModuleId, limit: u64) {
        if self.instance_map.get(&module_id).is_none() {
            panic!("Module not in the stack: {module_id:?}");
        }

        let (_, count) = self.instance_map.get_mut(&module_id).unwrap();
        *count += 1;

        self.stack.push(StackElement { module_id, limit });
    }

    /// Pops an element from the callstack.
    ///
    /// If it is a the last element in the stack for the popped instance.
    pub fn pop(&mut self) {
        if let Some(element) = self.stack.pop() {
            let mut entry = match self.instance_map.entry(element.module_id) {
                Entry::Occupied(e) => e,
                _ => unreachable!("map must have an instance here"),
            };

            let (instance, count) = entry.get_mut();
            *count -= 1;

            if *count == 0 {
                // SAFETY: This is the last instance of the module in the
                // stack, therefore we should recoup the memory and drop
                //
                // Any pointers to it will be left dangling
                unsafe {
                    let _ = Box::from_raw(*instance);
                    entry.remove();
                };
            }
        }
    }

    /// Returns a view of the stack to the `n`th element from the top.
    ///
    /// # Safety
    /// The reference to the instance available in the returned element is only
    /// guaranteed to be valid before the stack is called.
    pub fn nth_from_top<'a>(&self, n: usize) -> Option<StackElementView<'a>> {
        let len = self.stack.len();

        if len > n {
            let elem = &self.stack[len - (n + 1)];

            let (instance, _) = self.instance_map.get(&elem.module_id).unwrap();
            // SAFETY: We guarantee that the instance exists since we're in
            // control over if it is dropped in `pop`
            let instance = unsafe { &mut **instance };

            Some(StackElementView {
                module_id: elem.module_id,
                instance,
                limit: elem.limit,
            })
        } else {
            None
        }
    }

    /// Return the instance with the given module ID if it exists.
    pub fn instance<'a>(
        &self,
        module_id: &ModuleId,
    ) -> Option<&'a mut WrappedInstance> {
        self.instance_map.get(module_id).map(|(instance, _)| {
            // SAFETY: We guarantee that the instance exists since we're in
            // control over if it is dropped in `pop`
            unsafe { &mut **instance }
        })
    }
}

impl Drop for CallStack {
    fn drop(&mut self) {
        for (_, (instance, _)) in self.instance_map.iter() {
            unsafe {
                let _ = Box::from_raw(*instance);
            }
        }
    }
}

pub struct StackElementView<'a> {
    pub module_id: ModuleId,
    pub instance: &'a mut WrappedInstance,
    pub limit: u64,
}

#[derive(Debug)]
struct StackElement {
    module_id: ModuleId,
    limit: u64,
}
