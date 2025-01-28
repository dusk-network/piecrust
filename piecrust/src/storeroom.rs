// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Module which separates Piecrust logic from physical storage intricacies.
//! Users of 'storeroom' specify only what needs to be stored or retrieved, not
//! how. Main rationale for this module is to provide fully persistent model, in
//! a sense that persistence is multi-versioned.
//! Versions are fully independent, any change does not modify other versions.
//! Confluent model is not supported, version deletion is used instead, as old
//! versions are not needed after commits are finalized.

use std::io;
use std::path::{Path, PathBuf};

#[allow(dead_code)]
pub struct Storeroom {
    main_dir: PathBuf,
}

#[allow(dead_code)]
impl Storeroom {
    pub fn new(main_dir: impl AsRef<Path>) -> Self {
        Self { main_dir: main_dir.as_ref().to_path_buf() }
    }

    pub fn store_bytes(
        _bytes: &[u8],
        _contract_id: impl AsRef<str>,
        _version: impl AsRef<str>,
        _postfix: impl AsRef<str>,
    ) -> io::Result<()> {
        Ok(())
    }

    // For memory mapped files we also provide possibility to pass a file path
    pub fn store(
        _file_path: impl AsRef<Path>,
        _contract_id: impl AsRef<str>,
        _version: impl AsRef<str>,
        _postfix: impl AsRef<str>,
    ) -> io::Result<()> {
        Ok(())
    }

    pub fn retrieve_bytes(
        _contract_id: impl AsRef<str>,
        _version: impl AsRef<str>,
        _postfix: impl AsRef<str>,
    ) -> io::Result<Vec<u8>> {
        Ok(vec![])
    }

    // For memory mapped files we also provide retrieval returning a file path
    pub fn retrieve(
        _contract_id: impl AsRef<str>,
        _version: impl AsRef<str>,
        _postfix: impl AsRef<str>,
    ) -> io::Result<PathBuf> {
        Ok(PathBuf::new())
    }

    pub fn remove(_version: impl AsRef<str>) -> io::Result<()> {
        Ok(())
    }
}
