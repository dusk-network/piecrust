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

#[derive(Clone)]
pub struct MemoryHandler {
    memories: Arc<RwLock<BTreeMap<ModuleId, Linear>>>,
}

impl MemoryHandler {
    pub fn new() -> Self {
        MemoryHandler {
            memories: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    pub fn get_memory(&self, mod_id: ModuleId) -> Linear {
        {
            let rg = self.memories.read();
            if let Some(mem) = rg.get(&mod_id) {
                println!("clone");
                todo!()
                //return mem.clone();
            }
        }

        // TODO actually get it from the store
        let mem = Linear::new();
        println!("write");

        // todo
        // self.memories.write().insert(mod_id, mem.clone());

        println!("return from new");
        mem
    }
}
