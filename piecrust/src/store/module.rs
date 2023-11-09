// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::io;
use std::ops::Deref;
use std::path::Path;

use dusk_wasmtime::Engine;

/// WASM object code belonging to a given contract.
#[derive(Debug, Clone)]
pub struct Module {
    module: dusk_wasmtime::Module,
}

fn check_single_memory(module: &dusk_wasmtime::Module) -> io::Result<()> {
    // Ensure the module only has one memory
    let n_memories = module
        .exports()
        .filter_map(|exp| exp.ty().memory().map(|_| ()))
        .count();
    if n_memories != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "module has {} memories, but only one is allowed",
                n_memories
            ),
        ));
    }
    Ok(())
}

impl Module {
    pub(crate) fn new<B: AsRef<[u8]>>(
        engine: &Engine,
        bytes: B,
    ) -> io::Result<Self> {
        let module = unsafe {
            dusk_wasmtime::Module::deserialize(engine, bytes).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("failed to deserialize module: {}", e),
                )
            })?
        };

        check_single_memory(&module)?;

        Ok(Self { module })
    }

    pub(crate) fn from_file<P: AsRef<Path>>(
        engine: &Engine,
        path: P,
    ) -> io::Result<Self> {
        let module = unsafe {
            dusk_wasmtime::Module::deserialize_file(engine, path).map_err(
                |e| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("failed to deserialize module: {}", e),
                    )
                },
            )?
        };

        check_single_memory(&module)?;

        Ok(Self { module })
    }

    pub(crate) fn serialize(&self) -> Vec<u8> {
        self.module
            .serialize()
            .expect("We don't use WASM components")
    }

    pub(crate) fn is_64(&self) -> bool {
        self.module
            .exports()
            .filter_map(|exp| exp.ty().memory().map(|mem_ty| mem_ty.is_64()))
            .next()
            .expect("We guarantee the module has one memory")
    }
}

impl Deref for Module {
    type Target = dusk_wasmtime::Module;

    fn deref(&self) -> &Self::Target {
        &self.module
    }
}
