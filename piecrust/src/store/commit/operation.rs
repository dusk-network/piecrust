// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use crate::store::MAIN_DIR;
use crate::store::commit::finalizer::CommitFinalizer;
use crate::store::commit::remover::CommitRemover;
use crate::store::hasher::Hash;

const DELETE_FILE: &str = ".delete";
const FINALIZE_FILE: &str = ".finalize";
pub const COMPLETED_DIR: &str = ".completed-operations";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitOperation {
    Delete,
    Finalize,
}

impl CommitOperation {
    fn byte(self) -> u8 {
        match self {
            Self::Delete => 0,
            Self::Finalize => 1,
        }
    }

    fn filename(self) -> &'static str {
        match self {
            Self::Delete => DELETE_FILE,
            Self::Finalize => FINALIZE_FILE,
        }
    }

    pub fn pending<P: AsRef<Path>>(
        root_dir: P,
        root: Hash,
    ) -> io::Result<Option<Self>> {
        let delete = marker_exists(operation_path(
            root_dir.as_ref(),
            root,
            Self::Delete,
        ))?;
        let finalize = marker_exists(operation_path(
            root_dir.as_ref(),
            root,
            Self::Finalize,
        ))?;
        match (delete, finalize) {
            (false, false) => Ok(None),
            (true, false) => Ok(Some(Self::Delete)),
            (false, true) => Ok(Some(Self::Finalize)),
            (true, true) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Commit has conflicting pending operations",
            )),
        }
    }

    pub fn begin<P: AsRef<Path>>(
        root_dir: P,
        root: Hash,
        operation: Self,
    ) -> io::Result<()> {
        if let Some(pending) = Self::pending(&root_dir, root)? {
            return if pending == operation {
                Ok(())
            } else {
                Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("Commit already has pending {pending:?} operation"),
                ))
            };
        }

        let commit_dir =
            root_dir.as_ref().join(MAIN_DIR).join(hex::encode(root));
        let path = commit_dir.join(operation.filename());
        // The operation is encoded in the filename, so there is no marker
        // payload that can be observed partially written after a process
        // interruption.
        OpenOptions::new().write(true).create_new(true).open(path)?;
        Ok(())
    }

    pub fn complete<P: AsRef<Path>>(
        root_dir: P,
        root: Hash,
        operation: Self,
    ) -> io::Result<()> {
        let root_dir = root_dir.as_ref();
        if Self::pending(root_dir, root)? != Some(operation) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Commit operation marker does not match completion",
            ));
        }
        let main_dir = root_dir.join(MAIN_DIR);
        let commit_dir = main_dir.join(hex::encode(root));
        let completed_dir = main_dir.join(COMPLETED_DIR);
        fs::create_dir_all(&completed_dir)?;
        let completed = completed_dir.join(format!(
            "{}-{}",
            operation.byte(),
            hex::encode(root)
        ));
        match fs::remove_dir_all(&completed) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        fs::rename(commit_dir, &completed)?;
        match fs::remove_dir_all(completed) {
            Ok(()) => {
                if let Err(error) = fs::remove_dir(&completed_dir) {
                    tracing::warn!(
                        ?error,
                        "Unable to clean commit-operation directory"
                    );
                }
            }
            Err(error) => {
                tracing::warn!(
                    ?error,
                    "Unable to clean completed commit operation"
                );
            }
        }
        Ok(())
    }

    pub fn recover_all<P: AsRef<Path>>(root_dir: P) -> io::Result<()> {
        let root_dir = root_dir.as_ref();
        let main_dir = root_dir.join(MAIN_DIR);
        fs::create_dir_all(&main_dir)?;

        let completed = main_dir.join(COMPLETED_DIR);
        match fs::remove_dir_all(&completed) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => tracing::warn!(
                ?error,
                "Unable to clean completed commit operations"
            ),
        }

        let mut pending = Vec::new();
        for entry in fs::read_dir(&main_dir)? {
            let entry = entry?;
            if !entry.path().is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.len() != 64 {
                continue;
            }
            let root = decode_root(&name)?;
            if let Some(operation) = Self::pending(root_dir, root)? {
                pending.push((root, operation));
            }
        }

        for (root, operation) in pending {
            match operation {
                Self::Delete => CommitRemover::resume(root_dir, root)?,
                Self::Finalize => CommitFinalizer::resume(root, root_dir)?,
            }
        }
        Ok(())
    }

    pub fn any_pending<P: AsRef<Path>>(root_dir: P) -> io::Result<bool> {
        let main_dir = root_dir.as_ref().join(MAIN_DIR);
        let entries = match fs::read_dir(main_dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(false);
            }
            Err(error) => return Err(error),
        };
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.len() != 64 {
                continue;
            }
            let root = decode_root(&name)?;
            if Self::pending(root_dir.as_ref(), root)?.is_some() {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

fn operation_path(
    root_dir: &Path,
    root: Hash,
    operation: CommitOperation,
) -> PathBuf {
    root_dir
        .join(MAIN_DIR)
        .join(hex::encode(root))
        .join(operation.filename())
}

fn marker_exists(path: PathBuf) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(true),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid commit-operation marker",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn decode_root(encoded: &str) -> io::Result<Hash> {
    let mut root = [0u8; 32];
    hex::decode_to_slice(encoded, &mut root)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(root.into())
}
