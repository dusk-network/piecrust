// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use blake3::hash;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use wasmer::Module;

use crate::error::Error;
use crate::instance::Store;
use crate::store::COMPILED_DIR;
use crate::Error::ModuleCacheError;

#[derive(Clone)]
pub struct WrappedModule {
    serialized: Arc<Vec<u8>>,
}

impl WrappedModule {
    pub fn new<B: AsRef<[u8]>, P: AsRef<Path>>(
        bytecode: B,
        dir: P,
    ) -> Result<Self, Error> {
        let store = Store::new_store();
        let module_key = hash(bytecode.as_ref());
        let compiled_hex = hex::encode(module_key.as_bytes());
        let compiled_dir = dir.as_ref().join(COMPILED_DIR);
        let mut compiled_path = compiled_dir.clone();
        fs::create_dir_all(compiled_dir)
            .map_err(|err| ModuleCacheError(Arc::new(err)))?;
        compiled_path.push(compiled_hex);
        let serialized = match unsafe {
            Module::deserialize_from_file(
                &store,
                <PathBuf as AsRef<Path>>::as_ref(&compiled_path),
            )
        } {
            Ok(module) => module.serialize()?,
            _ => {
                let module = Module::new(&store, bytecode.as_ref())?;
                println!("dir={:?}", dir.as_ref());
                println!("compiled to file={:?}", compiled_path);
                module.serialize_to_file(<PathBuf as AsRef<Path>>::as_ref(
                    &compiled_path,
                ))?;
                module.serialize()?
            }
        };

        Ok(WrappedModule {
            serialized: Arc::new(serialized.to_vec()),
        })
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.serialized
    }
}
