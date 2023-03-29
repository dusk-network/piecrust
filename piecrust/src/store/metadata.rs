// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::io;
use std::path::Path;
use std::sync::Arc;

use rkyv::{archived_root, Deserialize, Infallible};

use crate::store::mmap::Mmap;
use crate::module::ModuleMetadata;

/// Module metadata pertaining to a given module but maintained by the host.
#[derive(Debug, Clone)]
pub struct Metadata {
    mmap: Arc<Mmap>,
    data: ModuleMetadata,
}

impl Metadata {
    pub(crate) fn new<B: AsRef<[u8]>>(
        bytes: B,
        data: ModuleMetadata,
    ) -> io::Result<Self> {
        let mmap = Mmap::new(bytes)?;

        Ok(Self {
            mmap: Arc::new(mmap),
            data,
        })
    }

    pub(crate) fn from_file<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let mmap = Mmap::map(path)?;
        let ret = unsafe { archived_root::<ModuleMetadata>(&mmap) };
        let data = ret.deserialize(&mut Infallible).expect("Infallible");

        Ok(Self {
            mmap: Arc::new(mmap),
            data,
        })
    }

    pub(crate) fn data(&self) -> &ModuleMetadata {
        &self.data
    }
}

impl AsRef<[u8]> for Metadata {
    fn as_ref(&self) -> &[u8] {
        &self.mmap
    }
}
