// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use crate::store::tree::Hash;
use bytecheck::CheckBytes;
use piecrust_uplink::ContractId;
use rkyv::{Archive, Deserialize, Serialize};
use std::path::Path;
use std::{fs, io};

#[derive(Debug, Clone, Default, Archive, Deserialize, Serialize)]
#[archive_attr(derive(CheckBytes))]
pub struct BaseInfo {
    pub contract_hints: Vec<ContractId>,
    pub maybe_base: Option<Hash>,
}

impl BaseInfo {
    pub fn from_path<P: AsRef<Path>>(path: P) -> io::Result<BaseInfo> {
        let path = path.as_ref();

        let base_info_bytes = fs::read(path)?;
        let base_info = rkyv::from_bytes(&base_info_bytes).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid base info file \"{path:?}\": {err}"),
            )
        })?;

        Ok(base_info)
    }
}
