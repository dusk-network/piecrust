// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::collections::btree_map::Entry::{Occupied, Vacant};
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::{io, mem};

use dusk_wasmtime::{Engine, Module};
use piecrust_uplink::ContractId;
use rusqlite::Result;

use crate::contract::ContractMetadata;
use crate::store::{
    Bytecode, Hash, Memory, Metadata, ModuleExt, StateStore, PAGE_SIZE,
};
use crate::Error;

#[derive(Debug, Clone)]
pub struct ContractDataEntry {
    pub wasm: Vec<u8>,
    pub module: Module,
    pub init_arg: Vec<u8>,
    pub owner: Vec<u8>,
    pub memory: Memory,
}

/// The representation of a session with a [`ContractStore`].
///
/// A session tracks modifications to the contracts' memories by keeping
/// references to the set of instantiated contracts.
///
/// The modifications are kept in memory and are only persisted to disk on a
/// call to [`commit`].
///
/// [`commit`]: ContractSession::commit
pub struct ContractSession {
    contracts: BTreeMap<ContractId, ContractDataEntry>,
    engine: Engine,

    store: mem::MaybeUninit<StateStore>,
    store_init: bool,
}

impl Debug for ContractSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContractSession")
            .field("contracts", &self.contracts)
            .finish()
    }
}

impl Drop for ContractSession {
    fn drop(&mut self) {
        if self.store_init {
            let mut store = mem::MaybeUninit::uninit();
            mem::swap(&mut store, &mut self.store);
            let _ = unsafe { store.assume_init() };
        }
    }
}

impl ContractSession {
    pub fn new(engine: Engine, store: StateStore) -> Self {
        Self {
            contracts: BTreeMap::new(),
            engine,
            store: mem::MaybeUninit::new(store),
            store_init: true,
        }
    }

    /// Sets the engine used for compilation/decompilation.
    pub fn set_engine(&mut self, engine: Engine) {
        self.engine = engine;
    }

    /// Commits the given session to disk, consuming the session and adding it
    /// to the [`ContractStore`] it was created from.
    ///
    /// Keep in mind that modifications to memories obtained using [`contract`],
    /// may cause the root to be inconsistent. The caller should ensure that no
    /// instance of [`Memory`] obtained via this session is being modified when
    /// calling this function.
    ///
    /// # Errors
    /// If this function is called twice.
    ///
    /// [`contract`]: ContractSession::contract
    pub fn commit(&mut self) -> io::Result<Hash> {
        if !self.store_init {
            return Err(io::Error::other("already committed this store"));
        }
        // TODO: Pump all contracts through to the store

        // move the store out of the
        let mut store = mem::MaybeUninit::uninit();
        mem::swap(&mut store, &mut self.store);
        let mut store = unsafe { store.assume_init() };

        self.store_init = false;

        let root = store.write_stored().map_err(io::Error::other)?;
        store.commit().map_err(io::Error::other)?;

        Ok(root)
    }

    /// Return the bytecode and memory belonging to the given `contract`, if it
    /// exists.
    ///
    /// The contract is considered to exist if either of the following
    /// conditions are met:
    ///
    /// - The contract has been [`deploy`]ed in this session
    /// - The contract was deployed to the base commit
    ///
    /// [`deploy`]: ContractSession::deploy
    pub fn contract(
        &mut self,
        contract: ContractId,
    ) -> io::Result<Option<ContractDataEntry>> {
        let contract_data = match self.contracts.entry(contract) {
            Vacant(entry) => {
                let contract = contract.to_bytes();
                match self
                    .store
                    .safe_assume_init()
                    .load_contract(contract)
                    .map_err(io::Error::other)?
                {
                    Some(contract_row) => {
                        // SAFETY: we trust the input from the database
                        let module = unsafe {
                            Module::deserialize(
                                &self.engine,
                                contract_row.native,
                            )
                            .map_err(io::Error::other)?
                        };

                        let mut store = self.store.safe_assume_init().clone();

                        let memory = Memory::with_pages(
                            module.is_64_bit(),
                            move |page_index: usize, page_buf: &mut [u8]| -> io::Result<usize> {
                                match store.load_page(contract, page_index as u64).map_err(io::Error::other)? {
                                    Some(page) => {
                                        page_buf.copy_from_slice(&page);
                                        Ok(PAGE_SIZE)
                                    }
                                    None => Ok(0),
                                }
                            },
                            contract_row.n_pages,
                        )?;

                        let contract_data = ContractDataEntry {
                            wasm: contract_row.wasm,
                            module,
                            init_arg: contract_row.init_arg,
                            owner: contract_row.owner,
                            memory,
                        };

                        entry.insert(contract_data).clone()
                    }
                    None => return Ok(None),
                }
            }
            Occupied(entry) => entry.get().clone(),
        };

        Ok(Some(contract_data))
    }

    /// Remove the given contract from the session.
    pub fn remove_contract(&mut self, contract: &ContractId) {
        self.contracts.remove(contract);
    }

    /// Deploys bytecode to the contract store with the given its `contract_id`.
    ///
    /// See [`deploy`] for deploying bytecode without specifying a contract ID.
    ///
    /// [`deploy`]: ContractSession::deploy
    pub fn deploy(
        &mut self,
        contract_id: ContractId,
        wasm: Vec<u8>,
        init_arg: Vec<u8>,
        owner: Vec<u8>,
    ) -> io::Result<()> {
        // If the contract already exists, return an error.
        if let Some(base) = self
            .store
            .safe_assume_init()
            .load_contract(contract_id.to_bytes())
            .map_err(io::Error::other)?
        {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Existing contract '{contract_id}'"),
            ));
        }

        let module =
            Module::new(&self.engine, &wasm).map_err(io::Error::other)?;
        let memory = Memory::new(module.is_64_bit())?;

        self.contracts.insert(
            contract_id,
            ContractDataEntry {
                wasm,
                module,
                init_arg,
                owner,
                memory,
            },
        );

        Ok(())
    }

    /// Remove the `old_contract` and move the `new_contract` to the
    /// `old_contract`, effectively replacing the `old_contract` with
    /// `new_contract`.
    pub fn replace(
        &mut self,
        old_contract: ContractId,
        new_contract: ContractId,
    ) -> Result<(), Error> {
        todo!("Replace a contract by another in the database");

        // let mut new_contract_data =
        //     self.contracts.remove(&new_contract).ok_or_else(|| {
        //         Error::PersistenceError(Arc::new(io::Error::new(
        //             io::ErrorKind::Other,
        //             format!("Contract '{new_contract}' not found"),
        //         )))
        //     })?;
        //
        // // The new contract should have the ID of the old contract in its
        // // metadata.
        // new_contract_data.metadata.set_data(ContractMetadata {
        //     contract_id: old_contract,
        //     owner: new_contract_data.metadata.data().owner.clone(),
        // })?;
        //
        // self.contracts.insert(old_contract, new_contract_data);
        //
        // Ok(())
    }
}

/// This allows us to only take the field, as opposed to the while struct.
trait SafeMaybeUninit<T>: Sized {
    fn safe_assume_init(&mut self) -> &mut T;
}

impl SafeMaybeUninit<StateStore> for mem::MaybeUninit<StateStore> {
    // SAFETY: we ensure that this is always set, by always dropping after
    // `fn commit`.
    fn safe_assume_init(&mut self) -> &mut StateStore {
        unsafe { self.assume_init_mut() }
    }
}
