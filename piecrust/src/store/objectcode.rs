// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::io;
use std::path::Path;
use std::sync::Arc;

use crate::store::mmap::Mmap;

/// WASM object code belonging to a given contract.
#[derive(Debug, Clone)]
pub struct Objectcode {
    mmap: Arc<Mmap>,
}

impl Objectcode {
    pub(crate) fn new<B: AsRef<[u8]>>(bytes: B) -> io::Result<Self> {
        let mmap = Mmap::new(bytes)?;

        Ok(Self {
            mmap: Arc::new(mmap),
        })
    }

    pub(crate) fn from_file<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let mmap = Mmap::map(path)?;
        Ok(Self {
            mmap: Arc::new(mmap),
        })
    }
}

impl AsRef<[u8]> for Objectcode {
    fn as_ref(&self) -> &[u8] {
        &self.mmap
    }
}
