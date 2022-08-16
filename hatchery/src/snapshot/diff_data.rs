// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use bytecheck::CheckBytes;
use rkyv::{
    ser::serializers::AllocSerializer, ser::Serializer, Archive, Deserialize,
    Serialize,
};

use crate::error::Error;
use crate::Error::PersistenceError;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::Path;

#[derive(Debug, Archive, Serialize, Deserialize)]
#[archive_attr(derive(CheckBytes, Debug))]
pub(crate) struct DiffData {
    original_len: usize,
    data: Vec<u8>,
}

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

    pub fn read<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let mut file =
            std::fs::File::open(path.as_ref()).map_err(PersistenceError)?;
        let metadata =
            std::fs::metadata(path.as_ref()).map_err(PersistenceError)?;
        let mut data = vec![0; metadata.len() as usize];
        file.read(data.as_mut_slice()).map_err(PersistenceError)?;
        let archived = rkyv::check_archived_root::<Self>(&data[..]).unwrap();
        let diff_data: Self =
            archived.deserialize(&mut rkyv::Infallible).unwrap();
        Ok(diff_data)
    }

    pub fn write<P: AsRef<Path>>(&self, path: P) -> Result<(), Error> {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .map_err(PersistenceError)?;

        let mut serializer = AllocSerializer::<0>::default();
        serializer.serialize_value(self).unwrap();
        let data = serializer.into_serializer().into_inner().to_vec();
        file.write(data.as_slice()).map_err(PersistenceError)?;
        Ok(())
    }
}

