// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use std::{io, mem};

use memmap2::{Mmap, MmapOptions};

use crate::contract::ContractMetadata;
use crate::{Error, Session};

/// Contract metadata pertaining to a given contract but maintained by the host.
#[derive(Debug, Clone)]
pub struct Metadata {
    mmap: Arc<Mmap>,
    data: ContractMetadata,
}

impl Metadata {
    pub(crate) fn new<B: AsRef<[u8]>>(
        bytes: B,
        data: ContractMetadata,
    ) -> io::Result<Self> {
        let bytes = bytes.as_ref();

        let mut mmap = MmapOptions::new().len(bytes.len()).map_anon()?;
        mmap.copy_from_slice(bytes);
        let mmap = mmap.make_read_only()?;

        Ok(Self {
            mmap: Arc::new(mmap),
            data,
        })
    }

    pub(crate) fn from_file<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = File::open(path)?;

        let mmap = unsafe { Mmap::map(&file)? };
        let data = rkyv::from_bytes(&mmap).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "contract metadata invalid in file",
            )
        })?;

        Ok(Self {
            mmap: Arc::new(mmap),
            data,
        })
    }

    pub(crate) fn data(&self) -> &ContractMetadata {
        &self.data
    }

    pub(crate) fn set_data(
        &mut self,
        data: ContractMetadata,
    ) -> Result<(), Error> {
        let bytes = Session::serialize_data(&data)?;

        let mut new = Self::new(bytes, data)
            .map_err(|err| Error::PersistenceError(Arc::new(err)))?;
        mem::swap(self, &mut new);

        Ok(())
    }
}

impl AsRef<[u8]> for Metadata {
    fn as_ref(&self) -> &[u8] {
        &self.mmap
    }
}
