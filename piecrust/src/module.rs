// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::sync::Arc;
use wasmer::Module;

use crate::error::Error;
use crate::instance::Store;

#[derive(Clone)]
pub struct WrappedModule {
    serialized: Arc<Vec<u8>>,
}

impl WrappedModule {
    pub fn new<B: AsRef<[u8]>, C: AsRef<[u8]>>(
        bytecode: B,
        objectcode: Option<C>,
    ) -> Result<Self, Error> {
        let store = Store::new_store();
        let serialized = match objectcode {
            Some(obj) => {
                println!("module restored");
                obj.as_ref().to_vec()
            }
            _ => {
                let module = Module::new(&store, bytecode.as_ref())?;
                println!("module compiled");
                module.serialize()?.to_vec()
            }
        };

        Ok(WrappedModule {
            serialized: Arc::new(serialized),
        })
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.serialized
    }
}
