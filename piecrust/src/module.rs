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
    pub fn new<B: AsRef<[u8]>>(bytecode: B) -> Result<Self, Error> {
        let store = Store::new_store();

        let module = wasmer::Module::new(&store, bytecode)?;
        Self::validate(&module)?;
        let serialized = module.serialize()?;

        Ok(WrappedModule {
            serialized: Arc::new(serialized.to_vec()),
        })
    }

    fn validate(module: &Module) -> Result<(), Error> {
        let required_exports = vec!["A", "SELF_ID", "memory"];
        let mut exports = vec![];
        for export in module.exports() {
            exports.push(export.name().into())
        }
        let mut missing_exports = vec![];
        for required_export in required_exports {
            if !exports.contains(&required_export.to_string()) {
                missing_exports.push(required_export);
            }
        }
        if missing_exports.is_empty() {
            Ok(())
        } else {
            Err(Error::ModuleValidationError(missing_exports.concat().into()))
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.serialized
    }
}
