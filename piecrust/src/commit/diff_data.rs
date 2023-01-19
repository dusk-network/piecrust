// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use bytecheck::CheckBytes;
use rkyv::{Archive, Deserialize, Serialize};

use crate::persistable::Persistable;

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
}

impl Persistable for DiffData {}
