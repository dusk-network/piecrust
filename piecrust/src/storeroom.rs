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

    fn get_item_path(
        &self,
        version: impl AsRef<str>,
        contract_id: impl AsRef<str>,
        item: impl AsRef<str>,
    ) -> io::Result<PathBuf> {
        let dir = self
            .main_dir
            .join(version.as_ref())
            .join(contract_id.as_ref());
        fs::create_dir_all(&dir)?;
        Ok(dir.join(item.as_ref()))
    }

    fn get_shared_item_path(
        &self,
        contract_id: impl AsRef<str>,
        item: impl AsRef<str>,
    ) -> PathBuf {
        self.main_dir
            .join(SHARED_DIR)
            .join(contract_id.as_ref())
            .join(item.as_ref())
    }

    pub fn store_bytes(
        &mut self,
        bytes: &[u8],
        version: impl AsRef<str>,
        contract_id: impl AsRef<str>,
        postfix: impl AsRef<str>,
    ) -> io::Result<()> {
        fs::write(self.get_item_path(version, contract_id, postfix)?, bytes)
    }

    // For memory mapped files we also provide possibility to pass a file path
    pub fn store(
        &mut self,
        source_path: impl AsRef<Path>,
        version: impl AsRef<str>,
        contract_id: impl AsRef<str>,
        postfix: impl AsRef<str>,
    ) -> io::Result<()> {
        fs::copy(
            source_path.as_ref(),
            self.get_item_path(contract_id, version, postfix)?,
        )
        .map(|_| ())
    }

    pub fn retrieve_bytes(
        &self,
        version: impl AsRef<str>,
        contract_id: impl AsRef<str>,
        item: impl AsRef<str>,
    ) -> io::Result<Option<Vec<u8>>> {
        let maybe_item_path = self.retrieve(
            version.as_ref(),
            contract_id.as_ref(),
            item.as_ref(),
        )?;
        if let Some(item_path) = maybe_item_path {
            Ok(Some(fs::read(item_path)?))
        } else {
            Ok(None)
        }
    }

    /// If item in version exists and it is not a blocking item it is returned
    /// If item in version exists and it is blocking item, none is returned
    /// If item in version does not exist, but in exists in SHARED dir, item
    /// from the shared dir is returned
    pub fn retrieve(
        &self,
        version: impl AsRef<str>,
        contract_id: impl AsRef<str>,
        item: impl AsRef<str>,
    ) -> io::Result<Option<PathBuf>> {
        let item_path = self.get_item_path(
            version.as_ref(),
            contract_id.as_ref(),
            item.as_ref(),
        )?;
        Ok(if item_path.is_file() {
            Some(item_path)
        } else if item_path.is_dir() {
            None
        } else {
            let shared_item_path =
                self.get_shared_item_path(contract_id.as_ref(), item.as_ref());
            if shared_item_path.is_file() {
                Some(shared_item_path)
            } else {
                None
            }
        })
    }

    /// Current shared version is the initial state
    pub fn create_version(&self, _version: impl AsRef<str>) -> io::Result<()> {
        Ok(())
    }

    #[rustfmt::skip]
    /*
    When finalizing, items are added or modified in shared version yet:
    for items added to the shared version:
        for every existing version other than this one,
            a blocking item is added
            (blocking item is item that blocks shared item, makes it
            as if didn't exist)
    for items modified in the shared version:
        before overwriting the existing item is copied to all existing
        versions other than this one, but only if it does not exist the
        corresponding version already
    After the above is done, the version can be safely deleted (not done here),
    new versions created in the future will use the
    shared version left by this function as their initial state
     */
    pub fn finalize_version(
        &mut self,
        version: impl AsRef<str>,
    ) -> io::Result<()> {
        let version_dir = self.main_dir.join(version.as_ref());
        if version_dir.is_dir() {
            for entry in fs::read_dir(version_dir)? {
                let entry = entry?;
                let contract_id =
                    entry.file_name().to_string_lossy().to_string();
                let contract_dir = entry.path();
                if contract_dir.is_dir() {
                    for entry in fs::read_dir(contract_dir)? {
                        let entry = entry?;
                        let item =
                            entry.file_name().to_string_lossy().to_string();
                        self.finalize_version_file(
                            version.as_ref(),
                            &contract_id,
                            item,
                        )?;
                    }
                }
            }
        }
        Ok(())
    }

    // for all versions
    //    if version does not have corresponding item
    //        create a block file for this item
    fn create_blocks_for(
        &mut self,
        finalized_version: impl AsRef<str>,
        contract_id: impl AsRef<str>,
        item: impl AsRef<str>,
    ) -> io::Result<()> {
        // for all versions
        for entry in fs::read_dir(&self.main_dir)? {
            let entry = entry?;
            let version = entry.file_name().to_string_lossy().to_string();
            if version == finalized_version {
                continue;
            }
            let version_dir = entry.path();
            let item_path =
                version_dir.join(contract_id.as_ref()).join(item.as_ref());
            if !item_path.exists() {
                // item being dir rather than file is treated as a blocker
                fs::create_dir_all(item_path)?;
            }
        }
        Ok(())
    }

    // for all versions
    //    if version does not have corresponding item
    //        copy item there as this is something that will be overwritten in
    // shared        so we want no change from the point of view of a
    // version
    fn create_copies_for(
        &mut self,
        finalized_version: impl AsRef<str>,
        contract_id: impl AsRef<str>,
        item: impl AsRef<str>,
        source_item_path: impl AsRef<Path>,
    ) -> io::Result<()> {
        // for all versions
        for entry in fs::read_dir(&self.main_dir)? {
            let entry = entry?;
            let version = entry.file_name().to_string_lossy().to_string();
            if version == finalized_version {
                continue;
            }
            let version_dir = entry.path();
            let item_path =
                version_dir.join(contract_id.as_ref()).join(item.as_ref());
            if !item_path.exists() {
                // copy item there as it will be overwritten soon
                fs::copy(&source_item_path, item_path)?;
            }
        }
        Ok(())
    }

    fn finalize_version_file(
        &mut self,
        version: impl AsRef<str>,
        contract_id: impl AsRef<str>,
        item: impl AsRef<str>,
    ) -> io::Result<()> {
        let shared_path = self.main_dir.join(SHARED_DIR);
        let shared_item_path =
            shared_path.join(contract_id.as_ref()).join(item.as_ref());
        let source_item_path = self
            .main_dir
            .join(version.as_ref())
            .join(contract_id.as_ref())
            .join(item.as_ref());
        if shared_item_path.is_file() {
            // shared item exists already, we need to copy it to existing
            // versions before overwriting
            self.create_copies_for(
                version.as_ref(),
                contract_id,
                item,
                &source_item_path,
            )?;
        } else {
            // shared item does not exist yet, we need to block existing
            // versions from using it
            self.create_blocks_for(version.as_ref(), contract_id, item)?;
        }
        fs::copy(source_item_path, shared_item_path)?;
        Ok(())
    }
}
