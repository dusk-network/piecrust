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

use std::path::{Path, PathBuf};
use std::{fs, io};

#[allow(dead_code)]
pub struct Storeroom {
    main_dir: PathBuf,
}

#[allow(dead_code)]
impl Storeroom {
    pub fn new(main_dir: impl AsRef<Path>) -> Self {
        Self {
            main_dir: main_dir.as_ref().to_path_buf(),
        }
    }

    fn get_target_path(
        &self,
        contract_id: impl AsRef<str>,
        version: impl AsRef<str>,
        postfix: impl AsRef<str>,
    ) -> io::Result<PathBuf> {
        let dir = self
            .main_dir
            .join(version.as_ref())
            .join(contract_id.as_ref());
        fs::create_dir_all(&dir)?;
        Ok(dir.join(postfix.as_ref()))
    }

    pub fn store_bytes(
        &mut self,
        bytes: &[u8],
        contract_id: impl AsRef<str>,
        version: impl AsRef<str>,
        postfix: impl AsRef<str>,
    ) -> io::Result<()> {
        fs::write(self.get_target_path(contract_id, version, postfix)?, bytes)
    }

    // For memory mapped files we also provide possibility to pass a file path
    pub fn store(
        &mut self,
        source_path: impl AsRef<Path>,
        contract_id: impl AsRef<str>,
        version: impl AsRef<str>,
        postfix: impl AsRef<str>,
    ) -> io::Result<()> {
        fs::copy(
            source_path.as_ref(),
            self.get_target_path(contract_id, version, postfix)?,
        )
        .map(|_| ())
    }

    pub fn retrieve_bytes(
        &self,
        _contract_id: impl AsRef<str>,
        _version: impl AsRef<str>,
        _postfix: impl AsRef<str>,
    ) -> io::Result<Vec<u8>> {
        Ok(vec![])
    }

    // For memory mapped files we also provide retrieval returning a file path
    pub fn retrieve(
        &self,
        _contract_id: impl AsRef<str>,
        _version: impl AsRef<str>,
        _postfix: impl AsRef<str>,
    ) -> io::Result<PathBuf> {
        Ok(PathBuf::new())
    }

    pub fn create_version(_version: impl AsRef<str>) -> io::Result<()> {
        Ok(())
    }

    pub fn finalize_version(_version: impl AsRef<str>) -> io::Result<()> {
        Ok(())
    }
}
