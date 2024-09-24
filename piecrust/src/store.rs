//! A piecrust store implementation.

#![deny(missing_docs)]
#![deny(clippy::pedantic)]

use std::collections::BTreeMap;
use std::mem;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use ouroboros::self_referencing;
use rusqlite::{
    params, CachedStatement, Connection, Error, OptionalExtension, Transaction,
};

mod bytecode;
mod memory;
mod metadata;
mod module;
mod session;

pub use bytecode::Bytecode;
pub use memory::Memory;
pub use metadata::Metadata;
pub use module::{Module, ModuleExt};
pub use session::{ContractDataEntry, ContractSession};

type Result<T, E = Error> = std::result::Result<T, E>;

pub const PAGE_SIZE: usize = 0x10000;

const HASH_SIZE: usize = 32;

pub type Hash = [u8; HASH_SIZE];
pub const ZERO_HASH: Hash = [0u8; HASH_SIZE];

const BITS: [u8; 8] = [
    0b10000000, 0b01000000, 0b00100000, 0b00010000, 0b00001000, 0b00000100,
    0b00000010, 0b00000001,
];
const MASKS: [u8; 8] = [
    0b10000000, 0b11000000, 0b11100000, 0b11110000, 0b11111000, 0b11111100,
    0b11111110, 0b11111111,
];

const CONTRACT_PATH_SIZE: usize = 32;
const PAGE_PATH_SIZE: usize = CONTRACT_PATH_SIZE + 8;

const CONTRACT_PATH_BITS: usize = 8 * CONTRACT_PATH_SIZE;
const PAGE_PATH_BITS: usize = 8 * PAGE_PATH_SIZE;

/// Contract as stored in the database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredContract {
    /// WebAssembly bytecode.
    pub wasm: Vec<u8>,
    /// Nativelly compiled code.
    pub native: Vec<u8>,
    /// The arguments that were used to initialize it.
    pub init_arg: Vec<u8>,
    /// The contract's owner.
    pub owner: Vec<u8>,
    /// The length of the memory of this contract.
    pub n_pages: u64,
}

/// A handle to the state, ready to perform any operations with it.
pub struct StateStore(&'static mut StateStoreInner);

unsafe impl Send for StateStore {}
unsafe impl Sync for StateStore {}

impl Drop for StateStore {
    fn drop(&mut self) {
        if self.0.ref_count.fetch_sub(1, Ordering::SeqCst) == 1 {
            unsafe {
                let _ = Box::from_raw(self.0);
            }
        }
    }
}

impl Clone for StateStore {
    /// Clone this handle to the state
    fn clone(&self) -> Self {
        self.0.ref_count.fetch_add(1, Ordering::SeqCst);

        let inner = self.0 as *const StateStoreInner;
        let inner = inner as *mut StateStoreInner;
        // SAFETY: we explicitly allow aliasing of the store for internal
        //         use, but this comes at the cost of ensuring downstream
        //         that it is only ever used by the same thread.
        Self(unsafe { &mut *inner })
    }
}

struct StateStoreInner {
    parent_root_node: NodeRow,
    transaction: StateStoreTransaction,

    // We keep the changed pages and contracts here, collecting them to be able
    // to insert nodes in a single shot, rather than having to modify them
    // later. The keys are both the size of a page path, to simplify downstream
    // operations.
    pages: BTreeMap<[u8; PAGE_PATH_SIZE], PageRow>,
    contracts: BTreeMap<[u8; PAGE_PATH_SIZE], ContractRow>,

    // The reference count for this store
    ref_count: AtomicUsize,
}

#[self_referencing]
struct StateStoreTransaction {
    conn: Connection,

    // This field might be `Option`, but is expected to be `Some` during the
    // lifetime of the struct. Its value is taken during a `commit` since
    // finalizing the transaction requires consuming it, and we would like to
    // signal any error to the caller.
    #[borrows(mut conn)]
    #[covariant]
    tx: Option<Transaction<'this>>,
}

impl StateStore {
    /// Open the state store. If a store does not exist at the path, one is
    /// created, and its initial state stored.
    ///
    /// # Errors
    /// Will return Err if path cannot be converted to a C-compatible string or
    /// if the underlying database `open` call fails.
    pub fn open(parent_root: Hash, path: impl AsRef<Path>) -> Result<Self> {
        let mut connection = Connection::open(path)?;
        Self::ensure_invariants(&mut connection)?;

        let inner =
            StateStoreTransaction::try_new::<Error>(connection, |conn| {
                Ok(Some(conn.transaction()?))
            })?;

        let mut this = StateStoreInner {
            parent_root_node: NodeRow::default(),
            transaction: inner,
            pages: BTreeMap::new(),
            contracts: BTreeMap::new(),
            ref_count: AtomicUsize::new(1),
        };

        let mut this = Self(Box::leak(Box::new(this)));

        this.0.parent_root_node = this.with_statements_mut(|stmts| {
            let parent_root = stmts.query_commit(parent_root)?;
            stmts.query_node(parent_root)
        })?;

        Ok(this)
    }

    /// Traverses the merkle tree to load the contents of a page. Returns `None`
    /// if the page was never inserted.
    ///
    /// # Errors
    /// If we fail to query the database.
    pub fn load_page(
        &mut self,
        contract: [u8; CONTRACT_PATH_SIZE],
        index: u64,
    ) -> Result<Option<[u8; PAGE_SIZE]>> {
        let mut current_node = self.0.parent_root_node;

        self.with_statements_mut(|stmts| {
            // The path along the merkle tree for the given page.
            let mut path = [0u8; PAGE_PATH_SIZE];

            path[..CONTRACT_PATH_SIZE].copy_from_slice(&contract);
            path[CONTRACT_PATH_SIZE..].copy_from_slice(&index.to_le_bytes());

            // Traverse through the entire tree until the leaf. If we encounter
            // an empty branch - there is no child along the path
            // specified by `bytes` - we immediatelly return `None`.
            for i in 0..PAGE_PATH_BITS {
                let byte_index = i / 8;
                let bit_index = i % 8;

                // if the bit is set we go right, otherwise we go left
                let child = if path[byte_index] & BITS[bit_index] == 0 {
                    current_node.lchild
                } else {
                    current_node.rchild
                };

                match child {
                    Some(child) => current_node = stmts.query_node(child)?,
                    None => return Ok(None),
                }
            }

            // Retrieve the contents of the page
            let page = match current_node.page {
                Some(page) => stmts.query_page(page)?,
                // This branch gets executed when the last page in the tree does
                // not contain a leaf entry. It is not something the database
                // can constrain, so we must ensure this logic
                // is correct at our level.
                None => return Err(Error::QueryReturnedNoRows),
            };

            Ok(Some(page.data))
        })
    }

    /// Traverses the merkle tree to load a contract. Returns `None` if the
    /// contract was never inserted.
    ///
    /// # Errors
    /// If we fail to query the database.
    pub fn load_contract(
        &mut self,
        contract: [u8; CONTRACT_PATH_SIZE],
    ) -> Result<Option<StoredContract>> {
        let mut current_node = self.0.parent_root_node;

        self.with_statements_mut(|stmts| {
            // The path along the merkle tree for the given contract.
            let path = contract;

            // Traverse through the tree until the contract node. If we
            // encounter an empty branch - there is no child along
            // the path specified by `bytes` - we immediatelly
            // return `None`.
            for i in 0..CONTRACT_PATH_BITS {
                let byte_index = i / 8;
                let bit_index = i % 8;

                // if the bit is set we go right, otherwise we go left
                let child = if path[byte_index] & BITS[bit_index] == 0 {
                    current_node.lchild
                } else {
                    current_node.rchild
                };

                match child {
                    Some(child) => current_node = stmts.query_node(child)?,
                    None => return Ok(None),
                }
            }

            // Retrieve the contents of the contract
            let contract = match current_node.contract {
                Some(contract) => stmts.query_contract(contract)?,
                // This branch gets executed when the contract node does not
                // contain a contract entry. It is not something
                // the database can constrain, so we must ensure
                // this logic is correct at our level.
                None => return Err(Error::QueryReturnedNoRows),
            };

            Ok(Some(StoredContract {
                wasm: contract.wasm,
                native: contract.native,
                init_arg: contract.init_arg,
                owner: contract.owner,
                n_pages: contract.n_pages,
            }))
        })
    }

    /// Stores a page.
    ///
    /// This doesn't call the database immediately, and thus cannot fail. It
    /// allows for a more batch-style insertion rather than a more expensive
    /// one-by-one insertion.
    ///
    /// To write the pages and contracts stored to the database, use the
    /// [`commit`] function.
    ///
    /// [`commit`]: StoreTransaction::commit
    pub fn store_page(
        &mut self,
        contract: [u8; CONTRACT_PATH_SIZE],
        index: u64,
        page: [u8; PAGE_SIZE],
    ) {
        let mut bytes = [0u8; PAGE_PATH_SIZE];
        bytes[..CONTRACT_PATH_SIZE].copy_from_slice(&contract);
        bytes[CONTRACT_PATH_SIZE..].copy_from_slice(&index.to_le_bytes());

        let mut hasher = blake3::Hasher::new();
        hasher.update(&page);
        let hash = hasher.finalize().into();

        self.0.pages.insert(bytes, PageRow { hash, data: page });
    }

    /// Stores a contract.
    ///
    /// This doesn't call the database immediately, and thus cannot fail. It
    /// allows for a more batch-style insertion rather than a more expensive
    /// one-by-one insertion.
    ///
    /// To write the pages and contracts stored to the database, use the
    /// [`commit`] function.
    ///
    /// [`commit`]: StoreTransaction::commit
    pub fn store_contract(
        &mut self,
        contract: [u8; CONTRACT_PATH_SIZE],
        wasm: Vec<u8>,
        native: Vec<u8>,
        init_arg: Vec<u8>,
        owner: Vec<u8>,
        n_pages: u64,
    ) {
        // the native code must not be included in the hash
        let mut hasher = blake3::Hasher::new();
        hasher.update(&wasm);
        hasher.update(&init_arg);
        hasher.update(&owner);
        hasher.update(&n_pages.to_le_bytes());
        let hash = hasher.finalize().into();

        let mut contract_path = [0u8; PAGE_PATH_SIZE];
        contract_path.copy_from_slice(&contract);

        self.0.contracts.insert(
            contract_path,
            ContractRow {
                hash,
                wasm,
                native,
                init_arg,
                owner,
                n_pages,
            },
        );
    }

    /// Write the pages and contracts stored using [`store_page`] and
    /// [`store_contract`] to the database, returning the resulting commit root.
    /// A write is atomic, i.e. if there is an error nothing is written, and if
    /// it is successful all stored pages and contracts are commited. This
    /// ensures database consistency, avoiding corrupted states.
    ///
    /// # Errors
    /// If we fail to write the commit to the database.
    pub fn write_stored(&mut self) -> Result<Hash> {
        let mut this_pages = BTreeMap::new();
        let mut this_contracts = BTreeMap::new();

        mem::swap(&mut this_pages, &mut self.0.pages);
        mem::swap(&mut this_contracts, &mut self.0.contracts);

        let parent_root_node = self.0.parent_root_node;

        self.with_statements_mut(|stmts| {
            let mut pages = BTreeMap::new();
            let mut contracts = BTreeMap::new();

            // Insert the pages into the database
            for (path, page) in this_pages {
                pages.insert(path, page.hash);
                stmts.insert_page(page)?;
            }

            // Insert the contracts into the database
            for (path, contract) in this_contracts {
                contracts.insert(path, contract.hash);
                stmts.insert_contract(contract)?;
            }

            // New nodes to be inserted into the tree separated by level,
            // ordered from root to leaves. To determine these we
            // first traverse the old tree, loading nodes along the
            // paths of the contracts and pages we want to insert.
            let mut new_nodes = Vec::with_capacity(PAGE_PATH_BITS + 1);

            for _ in 0..PAGE_PATH_BITS + 1 {
                new_nodes
                    .push(BTreeMap::<[u8; PAGE_PATH_SIZE], NodeRow>::new());
            }

            // insert the parent's root note into the list for ease of computing
            // downstream.
            new_nodes[0].insert([0u8; PAGE_PATH_SIZE], parent_root_node);

            // For each contract to be inserted, we load the paths on the old
            // tree since they will have to be inserted anew with
            // their hash recomputed and children changed.
            for (path, contract) in contracts {
                let mut node = parent_root_node;

                for i in 0..CONTRACT_PATH_BITS {
                    let byte_index = i / 8;
                    let bit_index = i % 8;

                    // if the bit is set we go right, otherwise we go left
                    let child = if path[byte_index] & BITS[bit_index] == 0 {
                        node.lchild
                    } else {
                        node.rchild
                    };

                    match child {
                        Some(child) => {
                            node = stmts.query_node(child)?;

                            let mut node_path = [0u8; PAGE_PATH_SIZE];
                            node_path[..byte_index]
                                .copy_from_slice(&path[..byte_index]);
                            node_path[byte_index] =
                                path[byte_index] & MASKS[bit_index];

                            new_nodes[i + 1].insert(node_path, node);
                        }
                        None => {
                            // When there are no more children along this path,
                            // we fill the maps with
                            // default rows, that will be filled
                            // with their proper values in the end.
                            for j in i..CONTRACT_PATH_BITS {
                                let byte_index = j / 8;
                                let bit_index = j % 8;

                                let mut node_path = [0u8; PAGE_PATH_SIZE];
                                node_path[..byte_index]
                                    .copy_from_slice(&path[..byte_index]);
                                node_path[byte_index] =
                                    path[byte_index] & MASKS[bit_index];

                                new_nodes[j + 1]
                                    .insert(node_path, NodeRow::default());
                            }

                            break;
                        }
                    }
                }

                new_nodes[CONTRACT_PATH_BITS]
                    .entry(path)
                    .or_insert(NodeRow::default())
                    .contract = Some(contract);
            }

            // For each page to be inserted, we load the paths on the old tree
            // since they will have to be inserted anew with their
            // hash recomputed and children changed.
            for (path, page) in pages {
                let mut node = parent_root_node;

                for i in 0..PAGE_PATH_BITS {
                    let byte_index = i / 8;
                    let bit_index = i % 8;

                    // if the bit is set we go right, otherwise we go left
                    let child = if path[byte_index] & BITS[bit_index] == 0 {
                        node.lchild
                    } else {
                        node.rchild
                    };

                    match child {
                        Some(child) => {
                            node = stmts.query_node(child)?;

                            let mut node_path = [0u8; PAGE_PATH_SIZE];
                            node_path[..byte_index]
                                .copy_from_slice(&path[..byte_index]);
                            node_path[byte_index] =
                                path[byte_index] & MASKS[bit_index];

                            new_nodes[i + 1].insert(node_path, node);
                        }
                        None => {
                            // When there are no more children along this path,
                            // we fill the maps with
                            // default rows, that will be filled
                            // with their proper values in the end.
                            for j in i..PAGE_PATH_BITS {
                                let byte_index = j / 8;
                                let bit_index = j % 8;

                                let mut node_path = [0u8; PAGE_PATH_SIZE];
                                node_path[..byte_index]
                                    .copy_from_slice(&path[..byte_index]);
                                node_path[byte_index] =
                                    path[byte_index] & MASKS[bit_index];

                                new_nodes[j + 1]
                                    .insert(node_path, NodeRow::default());
                            }

                            break;
                        }
                    }
                }

                // insert or modify the new page nodes into the map
                let node = new_nodes[PAGE_PATH_BITS]
                    .entry(path)
                    .or_insert(NodeRow::default());

                node.hash = page;
                node.page = Some(page);
            }

            // Here we go about inserting the nodes at each level and
            // recomputing the hashes of the parents. We always use
            // `unwrap()` on `pop` since we're sure the level was
            // inserted in the first place.
            let mut nodes = new_nodes.pop().unwrap();
            for i in (0..PAGE_PATH_BITS).rev() {
                let byte_index = i / 8;
                let bit_index = i % 8;

                let mut parent_nodes = new_nodes.pop().unwrap();

                for (parent_path, parent) in parent_nodes.iter_mut() {
                    let lchild_path = *parent_path;

                    let mut rchild_path = *parent_path;
                    rchild_path[byte_index] |= BITS[bit_index];

                    parent.lchild =
                        nodes.get(&lchild_path).map(|node| node.hash);
                    parent.rchild =
                        nodes.get(&rchild_path).map(|node| node.hash);

                    let mut hasher = blake3::Hasher::new();

                    hasher.update(parent.lchild.as_ref().unwrap_or(&ZERO_HASH));
                    hasher.update(parent.rchild.as_ref().unwrap_or(&ZERO_HASH));

                    if let Some(contract) = &parent.contract {
                        hasher.update(contract);
                    }

                    parent.hash = hasher.finalize().into();
                }

                for (_, node) in nodes {
                    stmts.insert_node(node)?;
                }

                nodes = parent_nodes;
            }

            // We're sure that there is exactly one node here - the root
            let root = nodes.remove(&[0u8; PAGE_PATH_SIZE]).unwrap();

            stmts.insert_node(root)?;
            stmts.insert_commit(root.hash)?;

            Ok(root.hash)
        })
    }

    /// Deletes the given commit from the store, and cleans up any orphaned
    /// contracts and pages.
    pub fn delete_commit(&mut self, root: Hash) -> Result<()> {
        // never delete the genesis commit
        if root == ZERO_HASH {
            return Ok(());
        }

        self.with_statements_mut(|stmts| {
            // if the commit doesn't exist we don't do anything
            if let Some(root) = stmts.query_optional_commit(root)? {
                stmts.delete_commit(root)?;
                stmts.delete_tree_nodes(root)?;
                stmts.delete_orphan_contracts()?;
                stmts.delete_orphan_pages()?;
            }

            Ok(())
        })
    }

    /// Commit all the changes made to the store.
    pub fn commit(self) -> Result<()> {
        self.0.transaction.with_mut(|inner| {
            let tx = inner.tx.take().expect(
                "`transaction` fields should be `Some` \
                  while the store handle exists",
            );
            tx.commit()
        })
    }

    /// Returns all the commits in the state.
    pub fn commits(&mut self) -> Result<Vec<Hash>> {
        self.with_statements_mut(|stmts| {
            let mut commits = stmts.query_commits()?;
            commits.retain(|commit| commit != &ZERO_HASH);
            Ok(commits)
        })
    }

    fn with_statements_mut<T, F: FnOnce(&mut Statements<'_>) -> Result<T>>(
        &mut self,
        closure: F,
    ) -> Result<T> {
        self.0.transaction.with_tx(|tx| {
            let tx = tx.as_ref().expect("`transaction` fields should be `Some` while the store handle exists");
            let mut stmts = Statements::new(tx)?;
            closure(&mut stmts)
        })
    }

    const CREATE_PAGES_TABLE: &str = "\
        CREATE TABLE IF NOT EXISTS pages ( \
            hash BLOB PRIMARY KEY, \
            data BLOB NOT NULL \
        ) STRICT;";

    const CREATE_CONTRACTS_TABLE: &str = "\
        CREATE TABLE IF NOT EXISTS contracts ( \
            hash     BLOB PRIMARY KEY, \
            wasm     BLOB NOT NULL, \
            native   BLOB NOT NULL, \
            init_arg BLOB NOT NULL, \
            owner    BLOB NOT NULL \
            n_pages  INTEGER NOT NULL \
        ) STRICT;";

    const CREATE_NODES_TABLE: &str = " \
        CREATE TABLE IF NOT EXISTS nodes ( \
            hash     BLOB PRIMARY KEY, \
            lchild   BLOB, \
            rchild   BLOB, \
            page     BLOB, \
            contract BLOB, \
            FOREIGN KEY (lchild)   REFERENCES nodes     (hash), \
            FOREIGN KEY (rchild)   REFERENCES nodes     (hash), \
            FOREIGN KEY (page)     REFERENCES pages     (hash), \
            FOREIGN KEY (contract) REFERENCES contracts (hash) \
        ) STRICT;";

    const CREATE_COMMITS_TABLE: &str = "\
        CREATE TABLE IF NOT EXISTS commits ( \
            root BLOB PRIMARY KEY, \
            FOREIGN KEY (root) REFERENCES nodes (hash) \
        ) STRICT;";

    const CREATE_CHILDREN_INDEX: &str = "\
        CREATE INDEX IF NOT EXISTS children \
        ON nodes (lchild, rchild)";

    const CREATE_PAGE_INDEX: &str = "\
        CREATE INDEX IF NOT EXISTS page \
        ON nodes (page)";

    const CREATE_CONTRACT_INDEX: &str = "\
        CREATE INDEX IF NOT EXISTS contract \
        ON nodes (contract)";

    const INSERT_GENESIS_NODE: &str = "\
        INSERT OR IGNORE INTO nodes (hash) \
        VALUES (?1)";

    const INSERT_GENESIS_COMMIT: &str = "\
        INSERT OR IGNORE INTO commits (root) \
        VALUES (?1)";

    /// This function ensures that the tables and indexes exist, and that there
    /// is at least a genesis root that can be built upon.
    fn ensure_invariants(conn: &mut Connection) -> Result<()> {
        let tx = conn.transaction()?;

        tx.execute(Self::CREATE_PAGES_TABLE, [])?;
        tx.execute(Self::CREATE_CONTRACTS_TABLE, [])?;
        tx.execute(Self::CREATE_NODES_TABLE, [])?;
        tx.execute(Self::CREATE_COMMITS_TABLE, [])?;

        tx.execute(Self::CREATE_CHILDREN_INDEX, [])?;
        tx.execute(Self::CREATE_PAGE_INDEX, [])?;
        tx.execute(Self::CREATE_CONTRACT_INDEX, [])?;

        tx.execute(Self::INSERT_GENESIS_NODE, [ZERO_HASH])?;
        tx.execute(Self::INSERT_GENESIS_COMMIT, [ZERO_HASH])?;

        tx.commit()
    }
}

#[derive(Default, Clone, Copy)]
struct NodeRow {
    hash: Hash,
    lchild: Option<Hash>,
    rchild: Option<Hash>,
    page: Option<Hash>,
    contract: Option<Hash>,
}

struct ContractRow {
    hash: Hash,
    wasm: Vec<u8>,
    native: Vec<u8>,
    init_arg: Vec<u8>,
    owner: Vec<u8>,
    n_pages: u64,
}

struct PageRow {
    hash: Hash,
    data: [u8; PAGE_SIZE],
}

struct Statements<'tx> {
    query_node: CachedStatement<'tx>,
    query_contract: CachedStatement<'tx>,
    query_page: CachedStatement<'tx>,
    query_commit: CachedStatement<'tx>,
    query_commits: CachedStatement<'tx>,
    insert_node: CachedStatement<'tx>,
    insert_contract: CachedStatement<'tx>,
    insert_page: CachedStatement<'tx>,
    insert_commit: CachedStatement<'tx>,
    delete_tree_nodes: CachedStatement<'tx>,
    delete_orphan_contracts: CachedStatement<'tx>,
    delete_orphan_pages: CachedStatement<'tx>,
    delete_commit: CachedStatement<'tx>,
}

impl<'tx> Statements<'tx> {
    const N_QUERY: &'static str =
        "SELECT hash, lchild, rchild, page, contract \
         FROM nodes \
         WHERE hash = ?1";

    const B_QUERY: &'static str =
        "SELECT hash, wasm, native, init_arg, owner, n_pages \
         FROM contracts \
         WHERE hash = ?1";

    const P_QUERY: &'static str = "SELECT hash, data \
                                   FROM pages \
                                   WHERE hash = ?1";

    const C_QUERY: &'static str = "SELECT root \
                                   FROM commits \
                                   WHERE root = ?1";

    const C_ALL_QUERY: &'static str = "SELECT root \
                                       FROM commits";

    const N_INSERT: &'static str = "INSERT OR IGNORE INTO nodes \
                                    (hash, lchild, rchild, page, contract) \
                                    VALUES(?1, ?2, ?3, ?4, ?5)";

    const B_INSERT: &'static str = "INSERT OR IGNORE INTO contracts \
                                    (hash, wasm, native, init_arg, owner) \
                                    VALUES(?1, ?2, ?3, ?4, ?5)";

    const P_INSERT: &'static str = "INSERT OR IGNORE INTO pages \
                                    (hash, data) \
                                    VALUES(?1, ?2)";

    const C_INSERT: &'static str = "INSERT OR IGNORE INTO commits \
                                    (root) \
                                    VALUES(?1)";

    const N_DELETE: &'static str = "WITH RECURSIVE to_delete AS ( \
                                           SELECT hash FROM nodes \
                                           WHERE hash = ?1 \
                                           UNION ALL \
                                           SELECT n.hash \
                                           FROM nodes n \
                                           JOIN to_delete child \
                                           ON child.hash = n.lchild OR child.hash = n.rchild \
                                           WHERE (SELECT COUNT(*) FROM nodes WHERE lchild = n.hash OR rchild = n.hash) < 2
                                         ) \
                                         DELETE FROM nodes \
                                         WHERE hash IN (SELECT hash FROM to_delete)";

    const B_DELETE: &'static str = "DELETE FROM contracts \
                                    WHERE hash NOT IN (SELECT DISTINCT contract FROM nodes WHERE contract IS NOT NULL)";

    const P_DELETE: &'static str = "DELETE FROM pages \
                                    WHERE hash NOT IN (SELECT DISTINCT page FROM nodes WHERE page IS NOT NULL)";

    const C_DELETE: &'static str = "DELETE FROM commits \
                                    WHERE root = ?1";

    fn new<'conn>(transaction: &'tx Transaction<'conn>) -> Result<Self> {
        Ok(Self {
            query_node: transaction.prepare_cached(Self::N_QUERY)?,
            query_contract: transaction.prepare_cached(Self::B_QUERY)?,
            query_page: transaction.prepare_cached(Self::P_QUERY)?,
            query_commit: transaction.prepare_cached(Self::C_QUERY)?,
            query_commits: transaction.prepare_cached(Self::C_ALL_QUERY)?,
            insert_node: transaction.prepare_cached(Self::N_INSERT)?,
            insert_contract: transaction.prepare_cached(Self::B_INSERT)?,
            insert_page: transaction.prepare_cached(Self::P_INSERT)?,
            insert_commit: transaction.prepare_cached(Self::C_INSERT)?,
            delete_tree_nodes: transaction.prepare_cached(Self::N_DELETE)?,
            delete_orphan_contracts: transaction
                .prepare_cached(Self::B_DELETE)?,
            delete_orphan_pages: transaction.prepare_cached(Self::P_DELETE)?,
            delete_commit: transaction.prepare_cached(Self::C_DELETE)?,
        })
    }

    fn query_node(&mut self, hash: Hash) -> Result<NodeRow> {
        self.query_node.query_row([hash], |row| {
            Ok(NodeRow {
                hash: row.get(0)?,
                lchild: row.get(1)?,
                rchild: row.get(2)?,
                page: row.get(3)?,
                contract: row.get(4)?,
            })
        })
    }

    fn query_contract(&mut self, hash: Hash) -> Result<ContractRow> {
        self.query_contract.query_row([hash], |row| {
            Ok(ContractRow {
                hash: row.get(0)?,
                wasm: row.get(1)?,
                native: row.get(2)?,
                init_arg: row.get(3)?,
                owner: row.get(4)?,
                n_pages: row.get(5)?,
            })
        })
    }

    fn query_page(&mut self, hash: Hash) -> Result<PageRow> {
        self.query_page.query_row([hash], |row| {
            Ok(PageRow {
                hash: row.get(0)?,
                data: row.get(1)?,
            })
        })
    }

    fn query_commit(&mut self, hash: Hash) -> Result<Hash> {
        self.query_commit.query_row([hash], |row| row.get(0))
    }

    fn query_optional_commit(&mut self, hash: Hash) -> Result<Option<Hash>> {
        self.query_commit
            .query_row([hash], |row| row.get(0))
            .optional()
    }

    fn query_commits(&mut self) -> Result<Vec<Hash>> {
        self.query_commits
            .query_map([], |row| row.get(0))?
            .into_iter()
            .collect()
    }

    fn insert_node(&mut self, node: NodeRow) -> Result<()> {
        self.insert_node
            .execute(params![
                node.hash,
                node.lchild,
                node.rchild,
                node.page,
                node.contract
            ])
            .map(|_| ())
    }

    fn insert_contract(&mut self, contract: ContractRow) -> Result<()> {
        self.insert_contract
            .execute(params![
                contract.hash,
                contract.wasm,
                contract.native,
                contract.init_arg,
                contract.owner
            ])
            .map(|_| ())
    }

    fn insert_page(&mut self, page: PageRow) -> Result<()> {
        self.insert_page
            .execute(params![page.hash, page.data])
            .map(|_| ())
    }

    fn insert_commit(&mut self, root: Hash) -> Result<()> {
        self.insert_commit.execute([root]).map(|_| ())
    }

    fn delete_tree_nodes(&mut self, root: Hash) -> Result<()> {
        self.delete_tree_nodes.execute([root]).map(|_| ())
    }

    fn delete_orphan_contracts(&mut self) -> Result<()> {
        self.delete_orphan_contracts.execute([]).map(|_| ())
    }

    fn delete_orphan_pages(&mut self) -> Result<()> {
        self.delete_orphan_pages.execute([]).map(|_| ())
    }

    fn delete_commit(&mut self, root: Hash) -> Result<()> {
        self.delete_commit.execute([root]).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_open() {
        let file = tempfile::NamedTempFile::new()
            .expect("Creating new temporary file should work");
        let _ = StateStore::open(ZERO_HASH, &file)
            .expect("Opening a connection should work");
    }

    #[test]
    fn empty_state() {
        let file = tempfile::NamedTempFile::new()
            .expect("Creating new temporary file should work");
        let mut store = StateStore::open(ZERO_HASH, &file)
            .expect("Opening a connection should succeed");

        let page = store
            .load_page([0u8; CONTRACT_PATH_SIZE], 42)
            .expect("Loading the page should succeed");

        assert!(page.is_none(), "There should be no pages in a fresh state");

        let contract = store
            .load_contract([0u8; CONTRACT_PATH_SIZE])
            .expect("Loading the contract should succeed");

        assert!(
            contract.is_none(),
            "There should be no contracts in a fresh state"
        );
    }

    #[test]
    fn store_and_commit() {
        let file = tempfile::NamedTempFile::new()
            .expect("Creating new temporary file should work");
        let mut store = StateStore::open(ZERO_HASH, &file)
            .expect("Opening a connection should succeed");

        let contract = [2u8; CONTRACT_PATH_SIZE];

        store.store_page(contract, 1, [100; PAGE_SIZE]);
        store.store_page(contract, 2, [2; PAGE_SIZE]);
        store.store_page(contract, 3, [3; PAGE_SIZE]);
        store.store_page(contract, 100, [100; PAGE_SIZE]);

        store.commit().expect("Committing should succeed");
    }

    #[test]
    fn store_commit_retrieve() {
        let file = tempfile::NamedTempFile::new()
            .expect("Creating new temporary file should work");
        let mut store = StateStore::open(ZERO_HASH, &file)
            .expect("Opening a connection should succeed");

        let contract = [2u8; CONTRACT_PATH_SIZE];

        store.store_page(contract, 1, [100; PAGE_SIZE]);
        store.store_page(contract, 2, [2; PAGE_SIZE]);

        let root = store.write_stored().expect("Writing should succeed");
        store.commit().expect("Committing should succeed");

        let mut store = StateStore::open(root, &file)
            .expect("Opening a connection should succeed");

        let page = store
            .load_page(contract, 1)
            .expect("Loading a page should succeed");

        assert_eq!(
            page,
            Some([100; PAGE_SIZE]),
            "The retrieved page should be the one previously inserted"
        );
    }

    #[test]
    fn store_commit_delete() {
        let file = tempfile::NamedTempFile::new()
            .expect("Creating new temporary file should work");
        let mut store = StateStore::open(ZERO_HASH, &file)
            .expect("Opening a connection should succeed");

        assert!(
            store
                .commits()
                .expect("Getting commits should succeed")
                .is_empty(),
            "Commits list should be empty"
        );

        let contract = [2u8; CONTRACT_PATH_SIZE];

        store.store_page(contract, 1, [100; PAGE_SIZE]);
        store.store_page(contract, 2, [2; PAGE_SIZE]);

        let root = store.write_stored().expect("Writing should succeed");
        store.commit().expect("Committing should succeed");

        let mut store = StateStore::open(root, &file)
            .expect("Opening a connection should succeed");

        assert_eq!(
            store
                .commits()
                .expect("Getting commits should succeed")
                .len(),
            1,
            "There should be one commit"
        );

        store
            .delete_commit(root)
            .expect("Deleting the commit should succeed");

        assert!(
            store
                .commits()
                .expect("Getting commits should succeed")
                .is_empty(),
            "Commits list should be empty"
        );

        assert!(StateStore::open(root, &file).is_err(),"Starting a transaction should fail with the deleted root as parent" );
    }
}
