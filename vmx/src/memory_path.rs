// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::error::Error::{self, PersistenceError};

#[derive(Debug)]
pub struct MemoryPath {
    path: PathBuf,
}

impl MemoryPath {
    pub fn new<P: AsRef<Path>>(path: P) -> Self
    where
        P: Into<PathBuf>,
    {
        MemoryPath { path: path.into() }
    }

    pub fn read(&self) -> Result<Vec<u8>, Error> {
        let mut f = std::fs::File::open(self.path.as_path())
            .map_err(PersistenceError)?;
        let metadata =
            std::fs::metadata(self.path.as_path()).map_err(PersistenceError)?;
        let mut buffer = vec![0; metadata.len() as usize];
        f.read(buffer.as_mut_slice()).map_err(PersistenceError)?;
        Ok(buffer)
    }
}

impl AsRef<Path> for MemoryPath {
    fn as_ref(&self) -> &Path {
        self.path.as_path()
    }
}
