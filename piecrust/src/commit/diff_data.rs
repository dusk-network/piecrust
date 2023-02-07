// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use bytecheck::CheckBytes;
use rkyv::{
    ser::serializers::{BufferScratch, CompositeSerializer, WriteSerializer},
    ser::Serializer,
    Archive, Deserialize, Serialize,
};

use crate::error::Error;
use crate::Error::PersistenceError;
use std::fs::OpenOptions;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Archive, Serialize, Deserialize)]
#[archive_attr(derive(CheckBytes, Debug))]
pub(crate) struct DiffData {
    original_len: usize,
    data: Vec<u8>,
}

const DIFF_DATA_SCRATCH_SIZE: usize = 64;

impl DiffData {
    pub fn new(original_len: usize, data: Vec<u8>) -> Self {
        Self { original_len, data }
    }

    pub fn original_len(&self) -> usize {
        self.original_len

    }

    pub fn data(&self) -> &[u8] {
        self.data.as_slice()
    }

    pub fn restore<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let mut file =
            std::fs::File::open(path.as_ref()).map_err(PersistenceError)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data).map_err(PersistenceError)?;
        let archived = rkyv::check_archived_root::<Self>(&data[..]).unwrap();
        let diff_data: Self =
            archived.deserialize(&mut rkyv::Infallible).unwrap();
        Ok(diff_data)
    }

    pub fn persist<P: AsRef<Path>>(&self, path: P) -> Result<(), Error> {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .map_err(PersistenceError)?;

        let mut scratch_buf = [0u8; DIFF_DATA_SCRATCH_SIZE];
        let scratch = BufferScratch::new(&mut scratch_buf);
        let serializer = WriteSerializer::new(file);
        let mut composite =
            CompositeSerializer::new(serializer, scratch, rkyv::Infallible);

        composite.serialize_value(self).unwrap();
        Ok(())
    }
}
