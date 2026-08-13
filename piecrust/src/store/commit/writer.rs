// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
#[cfg(all(test, target_os = "linux"))]
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::{fs, io};

use dusk_wasmtime::Engine;
use piecrust_uplink::ContractId;
use tracing::debug;

use crate::store::baseinfo::BaseInfo;
use crate::store::commit::Commit;
use crate::store::commit_store::CommitStore;
use crate::store::hasher::Hash;
use crate::store::session::ContractDataEntry;
use crate::store::{
    BASE_FILE, BYTECODE_DIR, Bytecode, ELEMENT_FILE, LEAF_DIR, MAIN_DIR,
    MEMORY_DIR, METADATA_EXTENSION, Module, OBJECTCODE_EXTENSION, PAGE_SIZE,
};

pub struct CommitWriter;

const SPARSE_BLOCK_SIZE: usize = 4 * 1024;
const SPARSE_MIN_HOLE_BLOCKS: usize = 2;
const UNPUBLISHED_PREFIX: &str = ".unpublished-";
const UNPUBLISHED_MAGIC: &[u8; 8] = b"PCRUSTU1";
const MEMORY_ENTRY: u8 = 1;
const LEAF_ENTRY: u8 = 2;

#[cfg(all(test, target_os = "linux"))]
static FAIL_DURING_BASE_PUBLICATION: AtomicBool = AtomicBool::new(false);

impl CommitWriter {
    ///
    /// Creates and writes commit, adds the created commit to commit store.
    /// The created commit is immutable and its hash (root) is calculated and
    /// returned by this method.
    pub fn create_and_write<P: AsRef<Path>>(
        root_dir: P,
        commit_store: Arc<Mutex<CommitStore>>,
        base: Option<Commit>,
        commit_contracts: BTreeMap<ContractId, ContractDataEntry>,
    ) -> io::Result<Hash> {
        let root_dir = root_dir.as_ref();

        let base_info = BaseInfo {
            maybe_base: base.as_ref().map(|base| *base.root()),
            ..Default::default()
        };

        let mut commit =
            base.unwrap_or(Commit::new(&commit_store, base_info.maybe_base));
        let mut identical_pages = BTreeMap::new();

        for (contract_id, contract_data) in &commit_contracts {
            let contract_identical_pages = commit.insert_contract(
                *contract_id,
                &contract_data.memory,
                contract_data.is_new,
            );
            if !contract_identical_pages.is_empty() {
                identical_pages.insert(*contract_id, contract_identical_pages);
            }
        }

        commit.squash();

        let root = *commit.root();
        let root_hex = hex::encode(root);
        commit.maybe_hash = Some(root);
        commit.base = base_info.maybe_base;

        // Don't write the commit if it already exists on disk. This may happen
        // if the same transactions on the same base commit for example.
        if commit_store.lock().unwrap().contains_key(&root) {
            return Ok(root);
        }

        Self::clear_unpublished_root(root_dir, &root_hex)?;
        let manifest = Self::unpublished_manifest(&commit, &commit_contracts);
        Self::write_unpublished_manifest(root_dir, root, &manifest)?;

        Self::write_commit_inner(
            root_dir,
            &commit,
            commit_contracts,
            identical_pages,
            root_hex,
            base_info,
        )
        .map(|_| {
            commit_store.lock().unwrap().insert_commit(root, commit);
            root
        })
    }

    /// Writes a commit to disk.
    fn write_commit_inner<P: AsRef<Path>, S: AsRef<str>>(
        root_dir: P,
        commit: &Commit,
        commit_contracts: BTreeMap<ContractId, ContractDataEntry>,
        identical_pages: BTreeMap<ContractId, BTreeSet<usize>>,
        commit_id: S,
        mut base_info: BaseInfo,
    ) -> io::Result<()> {
        let root_dir = root_dir.as_ref();

        struct Directories {
            main_dir: PathBuf,
            bytecode_main_dir: PathBuf,
            memory_main_dir: PathBuf,
            leaf_main_dir: PathBuf,
        }

        let directories = {
            let main_dir = root_dir.join(MAIN_DIR);
            fs::create_dir_all(&main_dir)?;

            let bytecode_main_dir = main_dir.join(BYTECODE_DIR);
            fs::create_dir_all(&bytecode_main_dir)?;

            let memory_main_dir = main_dir.join(MEMORY_DIR);
            fs::create_dir_all(&memory_main_dir)?;

            let leaf_main_dir = main_dir.join(LEAF_DIR);
            fs::create_dir_all(&leaf_main_dir)?;

            Directories {
                main_dir,
                bytecode_main_dir,
                memory_main_dir,
                leaf_main_dir,
            }
        };

        // Write the dirty pages contracts of contracts to disk.
        for (contract, contract_data) in &commit_contracts {
            let contract_hex = hex::encode(contract);

            let memory_main_dir =
                directories.memory_main_dir.join(&contract_hex);
            fs::create_dir_all(&memory_main_dir)?;

            let leaf_main_dir = directories.leaf_main_dir.join(&contract_hex);
            fs::create_dir_all(&leaf_main_dir)?;

            let commit_memory_dir = memory_main_dir.join(commit_id.as_ref());

            let mut dirty = false;
            // Write changed dirty pages. Identical dirty pages still mark the
            // contract as dirty so finalization and deletion process its leaf.
            for (dirty_page, _, page_index) in
                contract_data.memory.dirty_pages()
            {
                dirty = true;
                if identical_pages
                    .get(contract)
                    .is_some_and(|pages| pages.contains(page_index))
                {
                    continue;
                }

                let page_path: PathBuf = Self::page_path_main(
                    &memory_main_dir,
                    *page_index,
                    &commit_id,
                )?;
                Self::write_sparse_page(&page_path, dirty_page)?;
            }

            let bytecode_main_path =
                directories.bytecode_main_dir.join(&contract_hex);
            let module_main_path =
                bytecode_main_path.with_extension(OBJECTCODE_EXTENSION);
            let metadata_main_path =
                bytecode_main_path.with_extension(METADATA_EXTENSION);

            // If the contract is new, we write the bytecode, module, and
            // metadata files to disk.
            if contract_data.is_new {
                // we write them to the main location
                fs::write(bytecode_main_path, &contract_data.bytecode)?;
                contract_data.module.write_module_data(
                    module_main_path,
                    contract_data.bytecode.as_ref(),
                )?;
                fs::write(metadata_main_path, &contract_data.metadata)?;
                dirty = true;
            }
            if dirty {
                fs::create_dir_all(commit_memory_dir)?;
                base_info.contract_hints.push(*contract);
            }
        }

        tracing::trace!("persisting index started");
        for (contract_id, element) in commit.index.iter() {
            if commit_contracts.contains_key(contract_id) {
                let element_dir_path = directories
                    .leaf_main_dir
                    .join(hex::encode(contract_id.as_bytes()))
                    .join(commit_id.as_ref());
                let element_file_path = element_dir_path.join(ELEMENT_FILE);
                fs::create_dir_all(element_dir_path)?;
                let element_bytes =
                    rkyv::to_bytes::<_, 128>(element).map_err(|err| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("Failed serializing element file: {err}"),
                        )
                    })?;
                fs::write(&element_file_path, element_bytes)?;
            }
        }
        tracing::trace!("persisting index finished");

        let base_main_path =
            Self::base_path_main(&directories.main_dir, commit_id.as_ref())?;
        let base_info_bytes =
            rkyv::to_bytes::<_, 128>(&base_info).map_err(|err| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Failed serializing base info file: {err}"),
                )
            })?;
        Self::write_bytes_atomically(&base_main_path, &base_info_bytes)?;

        let manifest_path =
            Self::unpublished_manifest_path(root_dir, commit_id.as_ref());
        if let Err(error) = fs::remove_file(manifest_path) {
            if error.kind() != io::ErrorKind::NotFound {
                tracing::warn!(
                    ?error,
                    "Unable to clean unpublished-commit manifest"
                );
            }
        }

        Ok(())
    }

    fn page_path_main<P: AsRef<Path>, S: AsRef<str>>(
        memory_dir: P,
        page_index: usize,
        commit_id: S,
    ) -> io::Result<PathBuf> {
        let commit_id = commit_id.as_ref();
        let dir = memory_dir.as_ref().join(commit_id);
        fs::create_dir_all(&dir)?;
        Ok(dir.join(format!("{page_index}")))
    }

    fn write_sparse_page(path: &Path, page: &[u8]) -> io::Result<()> {
        if page.len() != PAGE_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Contract memory page has an invalid length",
            ));
        }

        let temporary = Self::temporary_path(path)?;
        let result = Self::write_sparse_page_inner(&temporary, page)
            .and_then(|()| fs::rename(&temporary, path));
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    fn write_sparse_page_inner(path: &Path, page: &[u8]) -> io::Result<()> {
        let zero_blocks = page
            .chunks(SPARSE_BLOCK_SIZE)
            .filter(|block| block.iter().all(|byte| *byte == 0))
            .count();

        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        if zero_blocks < SPARSE_MIN_HOLE_BLOCKS {
            file.write_all(page)?;
        } else {
            file.set_len(page.len() as u64)?;

            let mut offset = 0;
            while offset < page.len() {
                while offset < page.len()
                    && page[offset..]
                        .get(..SPARSE_BLOCK_SIZE)
                        .unwrap_or(&page[offset..])
                        .iter()
                        .all(|byte| *byte == 0)
                {
                    offset = (offset + SPARSE_BLOCK_SIZE).min(page.len());
                }
                let start = offset;
                while offset < page.len()
                    && page[offset..]
                        .get(..SPARSE_BLOCK_SIZE)
                        .unwrap_or(&page[offset..])
                        .iter()
                        .any(|byte| *byte != 0)
                {
                    offset = (offset + SPARSE_BLOCK_SIZE).min(page.len());
                }
                if start != offset {
                    file.seek(SeekFrom::Start(start as u64))?;
                    file.write_all(&page[start..offset])?;
                }
            }
        }

        if file.metadata()?.len() != page.len() as u64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Persisted contract memory page has an invalid length",
            ));
        }

        Ok(())
    }

    pub(crate) fn recover_unpublished_roots(root_dir: &Path) -> io::Result<()> {
        let main_dir = root_dir.join(MAIN_DIR);
        fs::create_dir_all(&main_dir)?;
        let mut manifests = Vec::new();
        let mut temporary_manifests = Vec::new();
        for entry in fs::read_dir(&main_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(root) = name.strip_prefix(UNPUBLISHED_PREFIX) {
                if root.len() == 64 {
                    manifests.push((root.to_owned(), entry.path()));
                }
            } else if name.starts_with(&format!(".{UNPUBLISHED_PREFIX}"))
                && name.ends_with(".piecrust-tmp")
            {
                temporary_manifests.push(entry.path());
            }
        }

        for (root, manifest_path) in manifests {
            match Self::clear_unpublished_root(root_dir, &root) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
            Self::remove_file_if_exists(&manifest_path)?;
        }
        for temporary in temporary_manifests {
            Self::remove_file_if_exists(&temporary)?;
        }
        Self::recover_legacy_unpublished_roots(root_dir)
    }

    // Commits written before root-local manifests were introduced can leave
    // unpublished deltas behind. Sweep those once on startup so an upgrade
    // cannot later resolve a reused root through stale legacy files. Normal
    // commit publication uses the bounded manifest path above.
    fn recover_legacy_unpublished_roots(root_dir: &Path) -> io::Result<()> {
        let main_dir = root_dir.join(MAIN_DIR);
        let mut published = BTreeMap::new();

        for entry in fs::read_dir(&main_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if !Self::is_root_name(&name) {
                continue;
            }
            let valid = match BaseInfo::from_path(entry.path().join(BASE_FILE))
            {
                Ok(_) => true,
                Err(error) if error.kind() == io::ErrorKind::NotFound => false,
                Err(error) => return Err(error),
            };
            published.insert(name.clone(), valid);
            if !valid {
                Self::remove_directory_if_exists(&entry.path())?;
            }
        }

        for namespace in [MEMORY_DIR, LEAF_DIR] {
            let namespace_dir = main_dir.join(namespace);
            let contracts = match fs::read_dir(namespace_dir) {
                Ok(contracts) => contracts,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    continue;
                }
                Err(error) => return Err(error),
            };
            for contract in contracts {
                let contract = contract?;
                if !contract.file_type()?.is_dir() {
                    continue;
                }
                for root in fs::read_dir(contract.path())? {
                    let root = root?;
                    if !root.file_type()?.is_dir() {
                        continue;
                    }
                    let name = root.file_name().to_string_lossy().to_string();
                    if !Self::is_root_name(&name) {
                        continue;
                    }
                    let is_published = if let Some(published) =
                        published.get(&name)
                    {
                        *published
                    } else {
                        let published_root = match BaseInfo::from_path(
                            main_dir.join(&name).join(BASE_FILE),
                        ) {
                            Ok(_) => true,
                            Err(error)
                                if error.kind() == io::ErrorKind::NotFound =>
                            {
                                false
                            }
                            Err(error) => return Err(error),
                        };
                        published.insert(name.clone(), published_root);
                        published_root
                    };
                    if !is_published {
                        Self::remove_directory_if_exists(&root.path())?;
                    }
                }
            }
        }
        Ok(())
    }

    fn is_root_name(name: &str) -> bool {
        name.len() == 64
            && hex::decode(name).is_ok_and(|bytes| bytes.len() == 32)
    }

    fn clear_unpublished_root(root_dir: &Path, root: &str) -> io::Result<()> {
        let main_dir = root_dir.join(MAIN_DIR);
        let commit_dir = main_dir.join(root);
        let base_path = commit_dir.join(BASE_FILE);
        let base_error = match BaseInfo::from_path(&base_path) {
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "State root is already published on disk",
                ));
            }
            Err(error) => error,
        };
        let manifest_path = Self::unpublished_manifest_path(root_dir, root);
        let manifest = match fs::read(&manifest_path) {
            Ok(bytes) => Some(Self::parse_unpublished_manifest(root, &bytes)?),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        if base_error.kind() != io::ErrorKind::NotFound && manifest.is_none() {
            return Err(base_error);
        }

        if let Some(entries) = manifest {
            for (contract, namespaces) in entries {
                let contract_hex = hex::encode(contract);
                if namespaces & MEMORY_ENTRY != 0 {
                    Self::remove_directory_if_exists(
                        &main_dir
                            .join(MEMORY_DIR)
                            .join(&contract_hex)
                            .join(root),
                    )?;
                }
                if namespaces & LEAF_ENTRY != 0 {
                    Self::remove_directory_if_exists(
                        &main_dir.join(LEAF_DIR).join(contract_hex).join(root),
                    )?;
                }
            }
        }

        Self::remove_directory_if_exists(&commit_dir)?;
        Self::remove_file_if_exists(&Self::temporary_path(&base_path)?)?;
        Self::remove_file_if_exists(&Self::temporary_path(&manifest_path)?)
    }

    fn unpublished_manifest(
        commit: &Commit,
        contracts: &BTreeMap<ContractId, ContractDataEntry>,
    ) -> BTreeMap<ContractId, u8> {
        contracts
            .iter()
            .filter_map(|(contract, data)| {
                let mut namespaces = 0;
                if data.is_new || data.memory.dirty_pages().next().is_some() {
                    namespaces |= MEMORY_ENTRY;
                }
                if commit.index.contains_key(contract) {
                    namespaces |= LEAF_ENTRY;
                }
                (namespaces != 0).then_some((*contract, namespaces))
            })
            .collect()
    }

    fn write_unpublished_manifest(
        root_dir: &Path,
        root: Hash,
        entries: &BTreeMap<ContractId, u8>,
    ) -> io::Result<()> {
        fs::create_dir_all(root_dir.join(MAIN_DIR))?;
        let mut bytes = Vec::with_capacity(48 + entries.len() * 33);
        bytes.extend_from_slice(UNPUBLISHED_MAGIC);
        bytes.extend_from_slice(root.as_bytes());
        bytes.extend_from_slice(&(entries.len() as u64).to_le_bytes());
        for (contract, namespaces) in entries {
            bytes.extend_from_slice(contract.as_bytes());
            bytes.push(*namespaces);
        }
        Self::write_bytes_atomically(
            &Self::unpublished_manifest_path(root_dir, &hex::encode(root)),
            &bytes,
        )
    }

    fn parse_unpublished_manifest(
        root: &str,
        bytes: &[u8],
    ) -> io::Result<BTreeMap<ContractId, u8>> {
        const HEADER_LEN: usize = 48;
        const ENTRY_LEN: usize = 33;
        if bytes.len() < HEADER_LEN || &bytes[..8] != UNPUBLISHED_MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid unpublished-commit manifest",
            ));
        }
        let expected_root = hex::decode(root).map_err(|error| {
            io::Error::new(io::ErrorKind::InvalidData, error)
        })?;
        if bytes[8..40] != expected_root {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Unpublished-commit manifest root mismatch",
            ));
        }
        let count =
            u64::from_le_bytes(bytes[40..48].try_into().map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid unpublished-commit manifest length",
                )
            })?);
        let count = usize::try_from(count).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Unpublished-commit manifest is too large",
            )
        })?;
        let expected_len = count
            .checked_mul(ENTRY_LEN)
            .and_then(|length| HEADER_LEN.checked_add(length))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Unpublished-commit manifest is too large",
                )
            })?;
        if bytes.len() != expected_len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid unpublished-commit manifest length",
            ));
        }

        let mut entries = BTreeMap::new();
        for entry in bytes[HEADER_LEN..].chunks_exact(ENTRY_LEN) {
            let contract = ContractId::from_bytes(
                entry[..32].try_into().map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Invalid unpublished-commit manifest entry",
                    )
                })?,
            );
            let namespaces = entry[32];
            if namespaces == 0
                || namespaces & !(MEMORY_ENTRY | LEAF_ENTRY) != 0
                || entries.insert(contract, namespaces).is_some()
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid unpublished-commit manifest entry",
                ));
            }
        }
        Ok(entries)
    }

    fn unpublished_manifest_path(root_dir: &Path, root: &str) -> PathBuf {
        root_dir
            .join(MAIN_DIR)
            .join(format!("{UNPUBLISHED_PREFIX}{root}"))
    }

    fn write_bytes_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
        let temporary = Self::temporary_path(path)?;
        let result = (|| {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            #[cfg(all(test, target_os = "linux"))]
            if path.file_name().is_some_and(|name| name == BASE_FILE)
                && FAIL_DURING_BASE_PUBLICATION.swap(false, Ordering::SeqCst)
            {
                file.write_all(&bytes[..bytes.len() / 2])?;
                return Err(io::Error::other(
                    "injected failure during base publication",
                ));
            }
            file.write_all(bytes)?;
            file.flush()?;
            drop(file);
            let persisted = fs::read(&temporary)?;
            if persisted != bytes {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Atomic publication temp has invalid contents",
                ));
            }
            fs::rename(&temporary, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    fn temporary_path(path: &Path) -> io::Result<PathBuf> {
        let filename = path.file_name().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Invalid file path")
        })?;
        Ok(path.with_file_name(format!(
            ".{}.piecrust-tmp",
            filename.to_string_lossy()
        )))
    }

    fn remove_file_if_exists(path: &Path) -> io::Result<()> {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn remove_directory_if_exists(path: &Path) -> io::Result<()> {
        match fs::remove_dir_all(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn base_path_main<P: AsRef<Path>, S: AsRef<str>>(
        main_dir: P,
        commit_id: S,
    ) -> io::Result<PathBuf> {
        let commit_id = commit_id.as_ref();
        let dir = main_dir.as_ref().join(commit_id);
        fs::create_dir_all(&dir)?;
        Ok(dir.join(BASE_FILE))
    }

    /// Remove the compiled module file for a given contract.
    ///
    /// This removes the object code file from disk, which will force
    /// recompilation when the contract is next loaded.
    pub fn remove_module<P: AsRef<Path>>(
        root_dir: P,
        contract_id: ContractId,
    ) -> io::Result<()> {
        let contract_hex = hex::encode(contract_id);
        let main_dir = root_dir.as_ref().join(MAIN_DIR);
        let bytecode_main_dir = main_dir.join(BYTECODE_DIR);
        let module_path = bytecode_main_dir
            .join(&contract_hex)
            .with_extension(OBJECTCODE_EXTENSION);

        Module::remove_cache_files(module_path)?;

        Ok(())
    }

    /// Recompile a module from its bytecode (overwrites the existing module).
    ///
    /// This reads the WASM bytecode from disk, recompiles it using the
    /// provided engine, and writes the compiled module back to disk.
    pub fn recompile_module<P: AsRef<Path>>(
        root_dir: P,
        engine: &Engine,
        contract_id: ContractId,
    ) -> io::Result<()> {
        let contract_hex = hex::encode(contract_id);
        let main_dir = root_dir.as_ref().join(MAIN_DIR);
        let bytecode_main_dir = main_dir.join(BYTECODE_DIR);

        let bytecode_path = bytecode_main_dir.join(&contract_hex);
        let module_path = bytecode_path.with_extension(OBJECTCODE_EXTENSION);

        // Check that bytecode exists
        if !bytecode_path.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Bytecode not found for contract: {contract_hex}"),
            ));
        }

        // Load bytecode and recompile
        let bytecode = Bytecode::from_file(&bytecode_path)?;
        let module = Module::from_bytecode(engine, bytecode.as_ref())
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;

        // Write the recompiled module
        module.write_module_data(module_path, bytecode.as_ref())?;
        debug!("Saved module for contract: {}", contract_hex);
        Ok(())
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::os::unix::fs::MetadataExt;

    use super::*;

    #[test]
    fn partial_base_publication_is_retryable_and_restartable() {
        use crate::{Error, SessionData, VM};

        let directory = tempfile::tempdir().unwrap();
        let vm = VM::new(directory.path()).unwrap();

        FAIL_DURING_BASE_PUBLICATION.store(true, Ordering::SeqCst);
        let first = vm.session(SessionData::builder()).unwrap().commit();
        assert!(
            matches!(first, Err(Error::PersistenceError(error)) if error.kind() == io::ErrorKind::Other),
            "the injected publication failure should be returned"
        );

        let retry = vm.session(SessionData::builder()).unwrap().commit();
        drop(vm);
        let restart = VM::new(directory.path());
        assert!(
            retry.is_ok() && restart.is_ok(),
            "retry={retry:?}, restart={restart:?}"
        );
    }

    #[test]
    fn unpublished_cleanup_ignores_unrelated_contract_namespaces() {
        let directory = tempfile::tempdir().unwrap();
        let root = format!("{:064x}", 1);
        let unrelated = directory
            .path()
            .join(MAIN_DIR)
            .join(MEMORY_DIR)
            .join(format!("{:064x}", 2))
            .join(&root);
        fs::create_dir_all(unrelated.parent().unwrap()).unwrap();
        fs::write(&unrelated, b"unrelated").unwrap();

        CommitWriter::clear_unpublished_root(directory.path(), &root)
            .expect("cleanup should not inspect unrelated contract parents");
        assert_eq!(fs::read(unrelated).unwrap(), b"unrelated");
    }

    #[test]
    fn startup_removes_legacy_unpublished_residue_but_keeps_published_roots() {
        let directory = tempfile::tempdir().unwrap();
        let main = directory.path().join(MAIN_DIR);
        let contract = format!("{:064x}", 7);
        let unpublished = format!("{:064x}", 8);
        let published = format!("{:064x}", 9);

        for root in [&unpublished, &published] {
            fs::create_dir_all(
                main.join(MEMORY_DIR).join(&contract).join(root),
            )
            .unwrap();
            fs::create_dir_all(main.join(LEAF_DIR).join(&contract).join(root))
                .unwrap();
            fs::create_dir_all(main.join(root)).unwrap();
        }
        let base = rkyv::to_bytes::<_, 128>(&BaseInfo::default()).unwrap();
        fs::write(main.join(&published).join(BASE_FILE), base).unwrap();

        CommitWriter::recover_unpublished_roots(directory.path()).unwrap();

        assert!(!main.join(&unpublished).exists());
        assert!(
            !main
                .join(MEMORY_DIR)
                .join(&contract)
                .join(&unpublished)
                .exists()
        );
        assert!(
            !main
                .join(LEAF_DIR)
                .join(&contract)
                .join(&unpublished)
                .exists()
        );
        assert!(main.join(&published).join(BASE_FILE).is_file());
        assert!(
            main.join(MEMORY_DIR)
                .join(&contract)
                .join(&published)
                .is_dir()
        );
        assert!(main.join(LEAF_DIR).join(contract).join(published).is_dir());
    }

    #[test]
    fn startup_rejects_ambiguous_legacy_base_corruption() {
        let directory = tempfile::tempdir().unwrap();
        let root = format!("{:064x}", 10);
        let commit = directory.path().join(MAIN_DIR).join(&root);
        fs::create_dir_all(&commit).unwrap();
        fs::write(commit.join(BASE_FILE), b"partial").unwrap();

        let error = CommitWriter::recover_unpublished_roots(directory.path())
            .expect_err("unmarked base corruption must fail closed");

        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(commit.join(BASE_FILE).is_file());
    }

    #[test]
    fn startup_clears_manifest_left_after_successful_publication() {
        let directory = tempfile::tempdir().unwrap();
        let root = [11; 32];
        let root_hex = hex::encode(root);
        CommitWriter::write_unpublished_manifest(
            directory.path(),
            root.into(),
            &BTreeMap::new(),
        )
        .unwrap();
        let manifest = CommitWriter::unpublished_manifest_path(
            directory.path(),
            &root_hex,
        );
        fs::write(&manifest, b"corrupt").unwrap();
        let commit = directory.path().join(MAIN_DIR).join(&root_hex);
        fs::create_dir_all(&commit).unwrap();
        let base = rkyv::to_bytes::<_, 128>(&BaseInfo::default()).unwrap();
        fs::write(commit.join(BASE_FILE), base).unwrap();

        CommitWriter::recover_unpublished_roots(directory.path()).unwrap();

        assert!(commit.join(BASE_FILE).is_file());
        assert!(!manifest.exists());
    }

    #[test]
    fn sparse_page_retains_length_and_contents_without_dense_allocation() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let path = temp.path().join("page");
        let mut page = vec![0; crate::store::PAGE_SIZE];
        page[2 * SPARSE_BLOCK_SIZE] = 1;
        page[10 * SPARSE_BLOCK_SIZE] = 2;

        CommitWriter::write_sparse_page(&path, &page)
            .expect("sparse page should be written");

        assert_eq!(fs::read(&path).expect("page should be readable"), page);
        let metadata = fs::metadata(path).expect("page metadata should exist");
        assert_eq!(metadata.len(), crate::store::PAGE_SIZE as u64);
        assert!(
            metadata.blocks() * 512 < metadata.len(),
            "zero page regions should remain filesystem holes"
        );
        assert!(
            !temp.path().join(".page.piecrust-tmp").exists(),
            "temporary page should be renamed after completion"
        );
    }

    #[test]
    fn all_zero_page_is_materialized_at_its_full_logical_length() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let path = temp.path().join("page");
        let page = vec![0; crate::store::PAGE_SIZE];

        CommitWriter::write_sparse_page(&path, &page)
            .expect("zero page should be written");

        assert_eq!(fs::metadata(&path).unwrap().len(), page.len() as u64);
        assert_eq!(fs::read(path).unwrap(), page);
    }

    #[test]
    fn complete_staged_page_replaces_a_stale_destination() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let path = temp.path().join("page");
        fs::write(&path, b"stale").expect("stale destination");
        let page = vec![3; crate::store::PAGE_SIZE];

        CommitWriter::write_sparse_page(&path, &page)
            .expect("completed temporary page should be published");

        assert_eq!(fs::read(path).unwrap(), page);
    }

    #[test]
    fn failed_page_publication_removes_the_temporary_file() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let path = temp.path().join("page");
        fs::create_dir(&path).expect("destination obstruction");
        let page = vec![1; crate::store::PAGE_SIZE];

        CommitWriter::write_sparse_page(&path, &page)
            .expect_err("a directory cannot be replaced by a page file");

        assert!(path.is_dir(), "the destination should remain untouched");
        assert!(
            !temp.path().join(".page.piecrust-tmp").exists(),
            "failed publication should remove its temporary file"
        );
    }
}
