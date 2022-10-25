// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use crate::error::Error;
use crate::instance::Store;

#[derive(Clone)]
pub struct WrappedModule {
    serialized: Vec<u8>,
}

impl WrappedModule {
    pub fn new(bytecode: &[u8]) -> Result<Self, Error> {
        let store = Store::new_store();

        let module = wasmer::Module::new(&store, bytecode)?;
        let serialized = module.serialize()?;

        Ok(WrappedModule {
            serialized: serialized.to_vec(),
        })
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.serialized
    }
}