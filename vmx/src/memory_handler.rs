// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::collections::BTreeMap;
use std::sync::Arc;

use parking_lot::RwLock;

use uplink::ModuleId;

use crate::linear::Linear;
use crate::vm::VM;

#[derive(Clone)]
pub struct MemoryHandler {
    memories: Arc<RwLock<BTreeMap<ModuleId, Linear>>>,
    #[allow(unused)]
    vm: VM,
}

impl MemoryHandler {
    pub fn new(vm: VM) -> Self {
        MemoryHandler {
            memories: Arc::new(RwLock::new(BTreeMap::new())),
            vm,
        }
    }

    pub fn get_memory(&self, mod_id: ModuleId) -> Linear {
        {
            let rg = self.memories.read();
            if let Some(mem) = rg.get(&mod_id) {
                return mem.clone();
            }
        }

        self.vm.with_module(mod_id, |module| {
            let mem = Linear::new(module.volatile().clone());
            self.memories.write().insert(mod_id, mem.clone());
            mem
        })
    }
}
