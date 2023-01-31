// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::collections::BTreeMap;
use std::path::Path;
use std::{fs, io};
use piecrust_uplink::{ModuleId, MODULE_ID_BYTES};
use crate::commit::{Hashable, ModuleCommitId};
use crate::module::WrappedModule;
use crate::vm::MODULES_DIR;
use crate::Error::{self, PersistenceError};

pub fn module_id_to_name(module_id: ModuleId) -> String {
    format!("{}", ByteArrayWrapper(module_id.as_bytes()))
}

pub fn commit_id_to_name(module_commit_id: ModuleCommitId) -> String {
    format!("{}", ByteArrayWrapper(module_commit_id.as_slice()))
}

pub struct ByteArrayWrapper<'a>(pub &'a [u8]);

impl<'a> core::fmt::UpperHex for ByteArrayWrapper<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if f.alternate() {
            write!(f, "0x")?
        }
        for byte in self.0 {
            write!(f, "{:02X}", &byte)?
        }
        Ok(())
    }
}

impl<'a> core::fmt::Display for ByteArrayWrapper<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::UpperHex::fmt(self, f)
    }
}
/// Parse a module ID and the file from the given `path`.
///
/// # Panics
/// If the given path doesn't have a final component, or that final component is
/// not valid UTF-8.
fn module_from_path<P: AsRef<Path>>(
    path: P,
) -> Result<(ModuleId, WrappedModule), Error> {
    let path = path.as_ref();
    let fname = path
        .file_name()
        .expect("The path must have a final component")
        .to_str()
        .expect("The final path component should be valid UTF-8");
    let module_id_bytes = hex::decode(fname).ok().ok_or_else(|| {
        PersistenceError(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid hex in file name",
        ))
    })?;
    if module_id_bytes.len() != MODULE_ID_BYTES {
        return Err(PersistenceError(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "Expected file name of length {MODULE_ID_BYTES}, found {}",
                module_id_bytes.len()
            ),
        )));
    }
    let mut bytes = [0u8; MODULE_ID_BYTES];
    bytes.copy_from_slice(&module_id_bytes);
    let module_id = ModuleId::from_bytes(bytes);
    let bytecode = fs::read(path).map_err(PersistenceError)?;
    let module = WrappedModule::new(&bytecode)?;
    Ok((module_id, module))
}
pub fn read_modules<P: AsRef<Path>>(
    base_path: P,
) -> Result<BTreeMap<ModuleId, WrappedModule>, Error> {
    let modules_dir = base_path.as_ref().join(MODULES_DIR);
    let mut modules = BTreeMap::new();
    // If the directory doesn't exist, then there are no modules
    if !modules_dir.exists() {
        return Ok(modules);
    }
    for entry in fs::read_dir(modules_dir).map_err(PersistenceError)? {
        let entry = entry.map_err(PersistenceError)?;
        let entry_path = entry.path();
        // Only read if it is a file, otherwise simply ignore
        if entry_path.is_file() {
            let (module_id, module) = module_from_path(entry_path)?;
            modules.insert(module_id, module);
        }
    }
    Ok(modules)
}
