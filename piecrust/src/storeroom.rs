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

const SHARED_DIR: &str = "SHARED";
const BLOCKING_FILE_STEM: &str = "BLOCKING";

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
    ) -> io::Result<Option<Vec<u8>>> {
        Ok(Some(vec![]))
    }

    /// If item in version exists and it is not a blocking item it is returned
    /// If item in version exists and it is blocking item, none is returned
    /// If item in version does not exist, but in exists in SHARED dir, item from the shared
    /// dir is returned
    // For memory mapped files we also provide retrieval returning a file path
    pub fn retrieve(
        &self,
        _contract_id: impl AsRef<str>,
        _version: impl AsRef<str>,
        _postfix: impl AsRef<str>,
    ) -> io::Result<Option<PathBuf>> {
        Ok(Some(PathBuf::new()))
    }

    /// Current shared version is the initial state
    pub fn create_version(_version: impl AsRef<str>) -> io::Result<()> {
        Ok(())
    }

    /// When finalizing, items are added or modified in shared version yet:
    /// for items added to the shared version:
    ///     for every existing version other than this one, a blocking item is added
    ///     (blocking item is item that blocks shared item, makes it as if didn't exist)
    /// for items modified in the shared version:
    ///     before overwriting the existing item is copied to all existing versions other than this one,
    ///     but only if it does not exist the corresponding version already
    /// After the above is done, version can be safely deleted, new versions created in the future
    /// will use the shared version left by this function as their initial state
    pub fn finalize_version(_version: impl AsRef<str>) -> io::Result<()> {
        Ok(())
    }
}
