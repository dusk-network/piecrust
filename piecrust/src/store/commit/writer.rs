// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use crate::store::baseinfo::BaseInfo;
use crate::store::commit::Commit;
use crate::store::commit_store::CommitStore;
use crate::store::hasher::Hash;
use crate::store::index::ContractIndexElement;
use crate::store::session::ContractDataEntry;
use crate::store::tree::{position_from_contract, ContractsMerkle};
use crate::store::treepos::TreePos;
use crate::store::{
    ContractSession, BASE_FILE, BYTECODE_DIR, ELEMENT_FILE, LEAF_DIR, MAIN_DIR,
    MEMORY_DIR, METADATA_EXTENSION, OBJECTCODE_EXTENSION,
};
use crate::Error;
use piecrust_uplink::ContractId;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::{fs, io};

pub struct CommitWriter;

type ContractMemTree = dusk_merkle::Tree<Hash, 32, 2>;

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

        // base is already a copy, no point cloning it again

        // let index = base
        //     .as_ref()
        //     .map_or(NewContractIndex::new(), |base| base.index.clone());
        // let contracts_merkle =
        //     base.as_ref().map_or(ContractsMerkle::default(), |base| {
        //         base.contracts_merkle.clone()
        //     });
        // let mut commit = Commit {
        //     index,
        //     contracts_merkle,
        //     maybe_hash: base.as_ref().and_then(|base| base.maybe_hash),
        // };

        let mut commit =
            base.unwrap_or(Commit::new(&commit_store, base_info.maybe_base));

        let mut new_elements =
            BTreeMap::<ContractId, ContractIndexElement>::new();
        for (contract_id, contract_data) in &commit_contracts {
            let element = if contract_data.is_new {
                commit.remove_and_insert(*contract_id, &contract_data.memory)
            } else {
                commit.insert(*contract_id, &contract_data.memory)
            };
            new_elements.insert(*contract_id, element.clone());
        }

        let root = *commit.root();
        let root_hex = hex::encode(root);
        commit.maybe_hash = Some(root);
        commit.base = base_info.maybe_base;

        // Don't write the commit if it already exists on disk. This may happen
        // if the same transactions on the same base commit for example.
        if commit_store.lock().unwrap().contains_key(&root) {
            return Ok(root);
        }

        let ret = Self::write_commit_inner(
            root_dir,
            &commit,
            commit_contracts,
            &root_hex,
            base_info.clone(),
            // &new_elements,
        )
        .map(|_| {
            commit_store
                .lock()
                .unwrap()
                .insert_commit(root, commit.clone());
            root
        });

        if let Ok(written_commit_hash) = ret {
            // todo: why is this not simply scanning all elements to verify?
            let elements_to_verify = scan_elements_from_commit(
                root_dir.join(MAIN_DIR),
                root_dir.join(MAIN_DIR).join(LEAF_DIR),
                Some(written_commit_hash),
            )?;

            let root_from_elements_ok =
                calc_root_from_elements(&elements_to_verify) == root;
            println!(
                "ROOT_OK_WITH_ELEMENTS={} {}",
                root_from_elements_ok,
                if root_from_elements_ok {
                    ""
                } else {
                    "(order matters)"
                }
            );
            println!(
                "ROOT_OK_WITH_TREE_POS={}",
                calc_root_from_tree_pos(commit.contracts_merkle.tree_pos())
                    == root
            );
            let _ = print_root_infos(
                &elements_to_verify,
                &new_elements,
                commit.contracts_merkle.tree_pos(),
            );
            println!("WRITTEN COMMIT {}  ======== (order matters)", root_hex);
        }

        ret
    }

    /// Writes a commit to disk.
    fn write_commit_inner<P: AsRef<Path>, S: AsRef<str>>(
        root_dir: P,
        commit: &Commit,
        commit_contracts: BTreeMap<ContractId, ContractDataEntry>,
        commit_id: S,
        mut base_info: BaseInfo,
        // new_elements: &BTreeMap<ContractId, ContractIndexElement>,
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

            let mut pages = BTreeSet::new();

            let mut dirty = false;
            // Write dirty pages and keep track of the page indices.
            for (dirty_page, _, page_index) in
                contract_data.memory.dirty_pages()
            {
                let page_path: PathBuf = Self::page_path_main(
                    &memory_main_dir,
                    *page_index,
                    &commit_id,
                )?;
                fs::write(page_path, dirty_page)?;
                pages.insert(*page_index);
                dirty = true;
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
                fs::write(module_main_path, contract_data.module.serialize())?;
                fs::write(metadata_main_path, &contract_data.metadata)?;
                dirty = true;
            }
            if dirty {
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
                println!("XWRITE ELEMENT {:?}", element_file_path);
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
        fs::write(base_main_path, base_info_bytes)?;

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

    fn base_path_main<P: AsRef<Path>, S: AsRef<str>>(
        main_dir: P,
        commit_id: S,
    ) -> io::Result<PathBuf> {
        let commit_id = commit_id.as_ref();
        let dir = main_dir.as_ref().join(commit_id);
        fs::create_dir_all(&dir)?;
        Ok(dir.join(BASE_FILE))
    }
}

fn scan_elements_from_commit(
    main_dir: impl AsRef<Path>,
    leaf_dir: impl AsRef<Path>,
    commit: Option<Hash>,
) -> io::Result<BTreeMap<ContractId, ContractIndexElement>> {
    let main_dir = main_dir.as_ref();
    let leaf_dir = leaf_dir.as_ref();
    let mut count = 0;
    let mut total_count = 0;
    let mut elements = BTreeMap::<ContractId, ContractIndexElement>::new();
    if let Some(hash) = commit {
        // for all contracts in leaf
        for entry in fs::read_dir(leaf_dir)? {
            let entry = entry?;
            if entry.path().is_dir() {
                let contract_id_hex =
                    entry.file_name().to_string_lossy().to_string();
                let contract_id = contract_id_from_hex(&contract_id_hex);
                let contract_leaf_path = leaf_dir.join(&contract_id_hex);
                if let Some((element_path, _)) = ContractSession::find_element(
                    Some(hash),
                    contract_leaf_path,
                    main_dir,
                    0,
                ) {
                    if element_path.is_file() {
                        if !elements.contains_key(&contract_id) {
                            count += 1;
                        }
                        println!(
                            "{} SCAN FOUND {:?} contains={}",
                            total_count,
                            element_path,
                            elements.contains_key(&contract_id)
                        );
                        let element_bytes = fs::read(&element_path)?;
                        let element: ContractIndexElement =
                            rkyv::from_bytes(&element_bytes).map_err(|err| {
                                tracing::trace!(
                                    "deserializing element file failed {}",
                                    err
                                );
                                io::Error::new(
                                    io::ErrorKind::InvalidData,
                                    format!(
                                        "Invalid element file \"{element_path:?}\": {err}"
                                    ),
                                )
                            })?;
                        total_count += 1;
                        elements.entry(contract_id).or_insert(element);
                    }
                }
            }
        }
    }
    println!("SCANNED {} ELEMENTS", count);
    println!("SCANNED {} TOTAL ELEMENTS", total_count);
    Ok(elements)
}

fn calc_root_from_elements(
    elements: &BTreeMap<ContractId, ContractIndexElement>,
) -> Hash {
    let mut merkle = ContractsMerkle::default();
    for (contract_id, element) in elements.iter() {
        merkle.insert_with_int_pos(
            position_from_contract(contract_id),
            element.int_pos().expect("int_pos should exist"),
            element.hash().expect("hash should exist"),
        );
    }
    let r = *merkle.root();
    r
}

fn calc_root_from_tree_pos(tree_pos: &TreePos) -> Hash {
    let mut merkle = ContractsMerkle::default();

    for (int_pos, (hash, pos)) in tree_pos.iter() {
        merkle.insert_with_int_pos(*pos, *int_pos as u64, *hash);
    }
    let r = *merkle.root();
    r
}

fn print_root_infos(
    elements: &BTreeMap<ContractId, ContractIndexElement>,
    new_elements: &BTreeMap<ContractId, ContractIndexElement>,
    tree_pos: &TreePos,
) -> Result<(), Error> {
    // println!();
    // println!("tree_pos");
    let mut tree_pos_map: HashMap<u64, [u8; 32]> = HashMap::new();
    for (k, (h, _c)) in tree_pos.iter() {
        // println!(
        //     "{} {} {}",
        //     *k,
        //     hex::encode(h),
        //     hex::encode((*c).to_le_bytes())
        // );
        tree_pos_map.insert(*k as u64, *h.as_bytes());
    }

    println!();
    println!("elems:");
    let mut sorted_elements: Vec<(ContractId, ContractIndexElement)> =
        elements.iter().map(|(a, b)| (*a, b.clone())).collect();
    sorted_elements.sort_by(|(_, el1), (_, el2)| {
        el1.int_pos()
            .expect("int_pos")
            .cmp(&el2.int_pos().expect("int_pos"))
    });
    for (contract_id, element) in sorted_elements.iter() {
        let is_new_element = new_elements.contains_key(contract_id);
        let contract_pos_hex =
            hex::encode(position_from_contract(contract_id).to_le_bytes());
        if Some(element.hash().expect("hash should exist").as_bytes())
            != tree_pos_map
                .get(&element.int_pos().expect("int_pos should exist"))
        {
            print!("* ");
            print!(
                "{} {} ({}) int_pos={} from tree_pos={} is_new={}",
                hex::encode(element.hash().expect("hash should exist")),
                contract_prefix(contract_id),
                contract_pos_hex,
                element.int_pos().expect("int_pos should exist"),
                hex::encode(
                    tree_pos_map
                        .get(&element.int_pos().expect("int_pos should exist"))
                        .expect("should be found")
                ),
                is_new_element,
            );
            println!();
        }
    }

    let root_from_elements = calculate_root(elements.iter().map(|(_, el)| {
        (
            *el.hash().expect("hash should exist").as_bytes(),
            el.int_pos().expect("int_pos should exist"),
        )
    }));
    println!();
    println!("root_from_elements={}", hex::encode(root_from_elements));
    let root_from_tree_pos_file = calculate_root_pos_32(
        tree_pos.iter().map(|(k, (h, _c))| (*h.as_bytes(), *k)),
    );
    println!();
    println!(
        "root_from_tree_pos_file={}",
        hex::encode(root_from_tree_pos_file)
    );

    println!();
    Ok(())
}

// todo: remove duplication, this fun is also in reader
fn contract_id_from_hex<S: AsRef<str>>(contract_id: S) -> ContractId {
    let bytes: [u8; 32] = hex::decode(contract_id.as_ref())
        .expect("Hex decoding of contract id string should succeed")
        .try_into()
        .expect("Contract id string conversion should succeed");
    ContractId::from_bytes(bytes)
}

fn contract_prefix(contract: &ContractId) -> String {
    let mut a = [0u8; 8];
    a.copy_from_slice(&contract.to_bytes()[..8]);
    hex::encode(a)
}

fn calculate_root(entries: impl Iterator<Item = ([u8; 32], u64)>) -> [u8; 32] {
    let mut tree = ContractMemTree::new();
    for (hash, int_pos) in entries {
        tree.insert(int_pos, hash);
    }
    let r = *(*tree.root()).as_bytes();
    r
}

fn calculate_root_pos_32(
    entries: impl Iterator<Item = ([u8; 32], u32)>,
) -> [u8; 32] {
    let mut tree = ContractMemTree::new();
    for (hash, int_pos) in entries {
        let int_pos = int_pos as u64;
        tree.insert(int_pos, hash);
    }
    let r = *(*tree.root()).as_bytes();
    r
}
