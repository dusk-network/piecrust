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

const SHARED_DIR: &str = "shared";

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
        item: impl AsRef<str>,
    ) -> io::Result<()> {
        fs::write(self.get_item_path(version, contract_id, item)?, bytes)
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
    pub fn create_version(&self, version: impl AsRef<str>) -> io::Result<()> {
        let version_path = self.main_dir.join(version.as_ref());
        fs::create_dir_all(&version_path)
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
    After the above is done, the version is safely deleted,
    new versions created in the future will use the
    shared version left by this function as their initial state
     */
    pub fn finalize_version(
        &mut self,
        version: impl AsRef<str>,
    ) -> io::Result<()> {
        let version_dir = self.main_dir.join(version.as_ref());
        if version_dir.is_dir() {
            for entry in fs::read_dir(&version_dir)? {
                let entry = entry?;
                let contract_id =
                    entry.file_name().to_string_lossy().to_string();
                if contract_id.starts_with(".") {
                    continue;
                }
                let contract_dir = entry.path();
                if contract_dir.is_dir() {
                    for entry in fs::read_dir(contract_dir)? {
                        let entry = entry?;
                        let item =
                            entry.file_name().to_string_lossy().to_string();
                        if item.starts_with(".") {
                            continue;
                        }
                        self.finalize_version_file(
                            version.as_ref(),
                            &contract_id,
                            item,
                        )?;
                    }
                }
            }
            fs::remove_dir_all(&version_dir)?;
        }
        Ok(())
    }

    // for all versions
    //    if version does not have corresponding item
    //        create a block file for this item
    fn create_blockings_for(
        &mut self,
        finalized_version: impl AsRef<str>,
        contract_id: impl AsRef<str>,
        item: impl AsRef<str>,
    ) -> io::Result<()> {
        // for all versions
        for entry in fs::read_dir(&self.main_dir)? {
            let entry = entry?;
            let version = entry.file_name().to_string_lossy().to_string();
            if version == finalized_version.as_ref() {
                continue;
            }
            if version.starts_with(".") {
                continue;
            }
            if version == SHARED_DIR {
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
            if version == finalized_version.as_ref() {
                continue;
            }
            if version.starts_with(".") {
                continue;
            }
            if version == SHARED_DIR {
                continue;
            }
            let version_dir = entry.path();
            let item_path =
                version_dir.join(contract_id.as_ref()).join(item.as_ref());
            if !item_path.exists() {
                // copy item there as it will be overwritten soon
                fs::copy(source_item_path.as_ref(), item_path)?;
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
        let shared_item_dir = shared_path.join(contract_id.as_ref());
        fs::create_dir_all(&shared_item_dir)?;
        let shared_item_path = shared_item_dir.join(item.as_ref());
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
                &shared_item_path,
            )?;
        } else {
            // shared item does not exist yet, we need to block existing
            // versions from using it
            self.create_blockings_for(version.as_ref(), contract_id, item)?;
        }
        if source_item_path.is_file() {
            fs::copy(source_item_path, shared_item_path)?;
        } else if source_item_path.is_dir() {
            // it is a blocking, we need to remove the shared path file as the
            // finalized version had a blocking on it
            fs::remove_file(shared_item_path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn storeroom_basic() -> Result<(), io::Error> {
        // const TEST_DIR: &str = "/Users/miloszm/storeroom";
        // fs::remove_dir_all(TEST_DIR)?;
        // fs::create_dir_all(TEST_DIR)?;
        let tmp_dir =
            tempdir().expect("Should be able to create temporary directory");

        let bytes1 = vec![1, 2, 3];
        let bytes2 = vec![4, 5, 6];
        let mut storeroom = Storeroom::new(tmp_dir);
        storeroom.store_bytes(&bytes1, "ver1", "aacc", "element")?;
        storeroom.store_bytes(&bytes2, "ver2", "aacc", "element")?;
        storeroom.create_version("ver3")?;
        storeroom.finalize_version("ver2")?;

        // check if finalize did not overwrite existing commits
        assert_eq!(storeroom.retrieve("ver3", "aacc", "element")?, None);
        let bytes_ver1 = storeroom.retrieve_bytes("ver1", "aacc", "element")?;
        assert_eq!(bytes_ver1, Some(bytes1));

        // from now on finalized content should be the default
        storeroom.create_version("ver4")?;
        let bytes_ver4 = storeroom.retrieve_bytes("ver4", "aacc", "element")?;
        assert_eq!(bytes_ver4, Some(bytes2));

        Ok(())
    }

    #[test]
    fn storeroom_update_shared() -> Result<(), io::Error> {
        // const TEST_DIR: &str = "/Users/miloszm/storeroom";
        // fs::remove_dir_all(TEST_DIR)?;
        // fs::create_dir_all(TEST_DIR)?;
        let tmp_dir =
            tempdir().expect("Should be able to create temporary directory");

        let bytes1 = vec![1, 2, 3];
        let bytes2 = vec![4, 5, 6];
        let mut storeroom = Storeroom::new(tmp_dir);
        storeroom.store_bytes(&bytes1, "ver1", "aacc", "element")?;
        storeroom.store_bytes(&bytes2, "ver2", "aacc", "element")?;
        storeroom.finalize_version("ver1")?;
        storeroom.create_version("ver3")?;
        assert_eq!(
            storeroom.retrieve_bytes("ver3", "aacc", "element")?,
            Some(bytes1.clone())
        );
        storeroom.finalize_version("ver2")?;
        storeroom.create_version("ver4")?;
        // ver3 should not be affected by the finalization of ver2
        assert_eq!(
            storeroom.retrieve_bytes("ver3", "aacc", "element")?,
            Some(bytes1)
        );
        assert_eq!(
            storeroom.retrieve_bytes("ver4", "aacc", "element")?,
            Some(bytes2)
        );

        Ok(())
    }

    #[test]
    fn storeroom_update_blocking() -> Result<(), io::Error> {
        // const TEST_DIR: &str = "/Users/miloszm/storeroom";
        // fs::remove_dir_all(TEST_DIR)?;
        // fs::create_dir_all(TEST_DIR)?;
        let tmp_dir =
            tempdir().expect("Should be able to create temporary directory");

        let bytes1 = vec![1, 2, 3];
        let mut storeroom = Storeroom::new(tmp_dir);
        storeroom.store_bytes(&bytes1, "ver1", "aacc", "element")?;
        storeroom.create_version("ver2")?;
        storeroom.finalize_version("ver1")?;
        storeroom.create_version("ver3")?;
        assert_eq!(
            storeroom.retrieve_bytes("ver3", "aacc", "element")?,
            Some(bytes1.clone())
        );
        storeroom.finalize_version("ver2")?;
        storeroom.create_version("ver4")?;
        // ver3 should not be affected by the finalization of ver2
        assert_eq!(
            storeroom.retrieve_bytes("ver3", "aacc", "element")?,
            Some(bytes1.clone())
        );
        assert_eq!(storeroom.retrieve_bytes("ver4", "aacc", "element")?, None);
        storeroom.store_bytes(&bytes1, "ver5", "aacc", "element")?;
        storeroom.finalize_version("ver5")?;
        assert_eq!(storeroom.retrieve_bytes("ver4", "aacc", "element")?, None);
        storeroom.finalize_version("ver3")?;
        assert_eq!(storeroom.retrieve_bytes("ver4", "aacc", "element")?, None);

        Ok(())
    }

    #[test]
    fn storeroom_misc() -> Result<(), io::Error> {
        // const TEST_DIR: &str = "/Users/miloszm/storeroom";
        // fs::remove_dir_all(TEST_DIR)?;
        // fs::create_dir_all(TEST_DIR)?;
        let tmp_dir =
            tempdir().expect("Should be able to create temporary directory");
        let bytes1 = vec![1, 2, 3];
        let bytes2 = vec![4, 5, 6];
        let bytes3 = vec![7, 8, 9];
        let bytes4 = vec![10, 11, 12];
        let bytes5 = vec![13, 14, 15];
        let mut storeroom = Storeroom::new(tmp_dir);
        storeroom.store_bytes(&bytes1, "ver1", "contract1", "element")?;
        storeroom.store_bytes(&bytes1, "ver1", "contract5", "element")?;
        storeroom.store_bytes(&bytes2, "ver2", "contract2", "element")?;
        storeroom.store_bytes(&bytes3, "ver3", "contract3", "element")?;
        storeroom.finalize_version("ver1")?;
        storeroom.store_bytes(&bytes4, "ver4", "contract4", "element")?;
        storeroom.store_bytes(&bytes5, "ver4", "contract1", "element")?;
        storeroom.finalize_version("ver4")?;
        storeroom.create_version("ver5")?;
        assert_eq!(
            storeroom.retrieve_bytes("ver5", "contract4", "element")?,
            Some(bytes4)
        );
        assert_eq!(
            storeroom.retrieve_bytes("ver5", "contract1", "element")?,
            Some(bytes5)
        );
        assert_eq!(
            storeroom.retrieve_bytes("ver5", "contract5", "element")?,
            Some(bytes1)
        );

        Ok(())
    }
}
