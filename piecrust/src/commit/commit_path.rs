// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::path::{Path, PathBuf};
use crate::commit::ModuleCommitLike;

#[derive(Debug)]
pub struct CommitPath {
    path: PathBuf,
}

impl CommitPath {
    pub fn new<P: AsRef<Path>>(path: P) -> Self
        where
            P: Into<PathBuf>,
    {
        CommitPath { path: path.into() }
    }
}

impl AsRef<Path> for CommitPath {
    fn as_ref(&self) -> &Path {
        self.path.as_path()
    }
}

impl ModuleCommitLike for CommitPath {
    fn path(&self) -> &PathBuf {
        &self.path
    }
}
