// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use crate::hash::{Hash, Hasher};

use std::cell::{Ref, RefCell, RefMut};
use std::collections::BTreeMap;
use std::fs;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::ptr::NonNull;

use dallo::ModuleId;
use memmap2::{MmapMut, MmapOptions};
use wasmer_types::{MemoryType, Pages, WASM_PAGE_SIZE};
use wasmer_vm::{LinearMemory, MemoryError, MemoryStyle, VMMemoryDefinition};

const PAGE_SIZE: usize = 65536;
const ZERO_HASH: [u8; 32] = [0u8; 32];
const ZEROED_PAGE: [u8; PAGE_SIZE] = [0u8; PAGE_SIZE];

/// The memory and whether it is dirty, wrapped in a `RefCell` for interior
/// mutability.
#[derive(Debug)]
struct MemRefCell(RefCell<(Memory, bool)>);

impl MemRefCell {
    fn new(mem: Memory, dirty: bool) -> Self {
        Self(RefCell::new((mem, dirty)))
    }

    fn borrow(&self) -> MemRef {
        MemRef(self.0.borrow())
    }

    fn borrow_mut(&self) -> MemRefMut {
        MemRefMut(self.0.borrow_mut())
    }
}

/// A reference to a memory.
#[derive(Debug)]
pub struct MemRef<'a>(Ref<'a, (Memory, bool)>);

impl<'a> MemRef<'a> {
    /// Returns true if the memory is 'dirty' - meaning it has been modified
    /// from the original.
    pub fn dirty(&self) -> bool {
        self.0.deref().1
    }
}

impl<'a> Deref for MemRef<'a> {
    type Target = Memory;

    fn deref(&self) -> &Self::Target {
        &self.0.deref().0
    }
}

/// A mutable reference to a memory.
#[derive(Debug)]
pub struct MemRefMut<'a>(RefMut<'a, (Memory, bool)>);

impl<'a> MemRefMut<'a> {
    /// Returns true if the memory is 'dirty' - meaning it has been modified
    /// from the original.
    pub fn dirty(&self) -> bool {
        self.0.deref().1
    }
}

impl<'a> Deref for MemRefMut<'a> {
    type Target = Memory;

    fn deref(&self) -> &Self::Target {
        &self.0.deref().0
    }
}

impl<'a> DerefMut for MemRefMut<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        let (mem, dirty) = self.0.deref_mut();
        *dirty = true;
        mem
    }
}

#[derive(Debug)]
pub struct MemorySession {
    memories: BTreeMap<ModuleId, MemRefCell>,
    dir: PathBuf,
    base: Option<Hash>,
}

impl MemorySession {
    /// Creates a new memory store at the specified `path` and the given `base`
    /// snapshot ID.
    ///
    /// If the directory doesn't exists it will be created.
    pub fn new<P: AsRef<Path>>(dir: P, base: Hash) -> io::Result<Self> {
        let dir = PathBuf::from(dir.as_ref());

        // create directory if it doesn't exist
        if !dir.exists() {
            fs::create_dir(&dir)?;
        }

        // if the base directory doesn't exist, then there is no base
        let base_hex = hex::encode(&base);
        let base = snapshot_dir(&dir, &base_hex).exists().then_some(base);

        Ok(Self {
            memories: BTreeMap::new(),
            dir,
            base,
        })
    }

    /// Borrow a memory for the given `module_id`, if it has already been
    /// [`load`]ed.
    ///
    /// If a memory with the same module ID exists at the base snapshot that
    /// memory will be loaded copy-on-write, otherwise a new memory will be
    /// created.
    ///
    /// # Panics
    /// When the memory with given `module_id` is already borrowed mutably using
    /// [`borrow_mut`].
    ///
    /// [`load`]: MemorySession::load
    /// [`borrow_mut`]: MemorySession::borrow_mut
    pub fn borrow(&self, module_id: &ModuleId) -> Option<MemRef> {
        self.memories.get(module_id).map(|m| m.borrow())
    }

    /// Get a mutable memory for the given `module_id`, if it has already been
    /// [`load`]ed.
    ///
    /// # Panics
    /// When the memory with given `module_id` is already borrowed using either
    /// [`borrow`] or [`borrow_mut`].
    ///
    /// [`load`]: MemorySession::load
    /// [`borrow`]: MemorySession::borrow
    /// [`borrow_mut`]: MemorySession::borrow_mut
    pub fn borrow_mut(&self, module_id: &ModuleId) -> Option<MemRefMut> {
        self.memories.get(module_id).map(|m| m.borrow_mut())
    }

    /// Loads a memory onto the store.
    ///
    /// If a memory with the same module ID exists following the base snapshot
    /// path, that memory will be loaded copy-on-write, otherwise a new memory
    /// will be created.
    pub fn load(&mut self, module_id: ModuleId) -> io::Result<()> {
        if self.memories.get(&module_id).is_none() {
            let mem = match self.last_module_snap(&module_id)? {
                Some(path) => Memory::new(path)?,
                None => Memory::ephemeral()?,
            };
            self.memories.insert(module_id, MemRefCell::new(mem, false));
        }

        Ok(())
    }

    /// Create a snapshot from the current state of the memories and rebase onto
    /// it, returning the snapshot ID - the root of the state.
    pub fn snap(&mut self) -> io::Result<Hash> {
        let snap = self.root();

        // if the snapshot is root is the same as the base - or genesis - return
        // it immediately
        if &snap == self.base.as_ref().unwrap_or(&Hash::ZERO) {
            return Ok(snap);
        }

        let snap_hex = hex::encode(&snap);

        // create snapshot directory if it does not exist
        let snap_dir = snapshot_dir(&self.dir, &snap_hex);
        if !snap_dir.exists() {
            fs::create_dir(snap_dir)?;
        }

        // create a file indicating this snapshot has a base
        if let Some(base) = &self.base {
            let base_path = snapshot_base_path(&self.dir, &snap_hex);
            fs::write(base_path, base)?;
        }

        // copy all dirty memories onto their respective files and mark them as
        // clean
        for (module_id, mem) in &mut self.memories {
            let mut mem_ref = mem.0.borrow_mut();
            let (mem, dirty) = mem_ref.deref_mut();

            if *dirty {
                let module_id_hex = hex::encode(module_id);
                let module_path =
                    module_path(&self.dir, &snap_hex, &module_id_hex);

                mem.copy_to(module_path)?;
                *dirty = false;
            }
        }

        self.base = Some(snap);

        Ok(snap)
    }

    /// Return the root of the module tree.
    ///
    /// The root of these memories is the previous root hashed with the root of
    /// a merkle tree where the leaves are the hashes of each changed memory +
    /// module ID. The memories are ordered in the tree by module ID.
    ///
    /// # Panics
    /// When any memory is mutably borrowed.
    pub fn root(&self) -> Hash {
        // !hash all the memories!
        let mut hashes = self
            .memories
            .iter()
            .filter_map(|(module_id, mem_cell)| {
                // filter out all the clean memories
                let mem_ref = mem_cell.0.borrow();
                let (_, dirty) = mem_ref.deref();
                dirty.then_some((module_id, mem_cell))
            })
            .map(|(module_id, mem_cell)| {
                let mem = mem_cell.borrow();
                let mut hasher = Hasher::new();

                hasher.update(module_id.as_ref());
                hasher.update(mem.as_ref());

                hasher.finalize()
            })
            .collect::<Vec<Hash>>();

        // if the tree is empty, we are still on the previous snapshot - or in
        // genesis.
        if hashes.is_empty() {
            return self.base.unwrap_or(Hash::ZERO);
        }

        // compute the root of the tree by successively hashing each level
        while hashes.len() > 1 {
            hashes = hashes
                .chunks(2)
                .map(|hashes| {
                    let mut hasher = Hasher::new();
                    for hash in hashes {
                        hasher.update(hash);
                    }
                    hasher.finalize()
                })
                .collect();
        }

        // hash the previous snapshot's root together with the merkle root
        let mut hasher = Hasher::new();

        hasher.update(self.base.unwrap_or(Hash::ZERO));
        hasher.update(hashes[0]);

        hasher.finalize()
    }

    /// Return the path to the last snapshot of a module.
    ///
    /// If there has never been a snapshot of the module in the snapshot path,
    /// `None` will be returned.
    fn last_module_snap(
        &self,
        module_id: &ModuleId,
    ) -> io::Result<Option<PathBuf>> {
        let module_hex = hex::encode(module_id);

        match &self.base {
            // If there is a base snapshot for the running store, drill down
            // through the snapshots until we find one with the given module ID.
            // If the module is not found in any snapshots in the path return
            // None.
            Some(mut base) => loop {
                let base_hex = hex::encode(base);
                let snap_dir = snapshot_dir(&self.dir, &base_hex);

                if snap_dir.exists() && snap_dir.is_dir() {
                    let module_path =
                        module_path(&self.dir, &base_hex, &module_hex);

                    if module_path.exists() && module_path.is_file() {
                        return Ok(Some(module_path));
                    }

                    let base_path = snapshot_base_path(&self.dir, &base_hex);
                    if base_path.exists() && base_path.is_file() {
                        let base_bytes = fs::read(base_path)?;
                        let base_bytes: [u8; 32] =
                            base_bytes.try_into().unwrap();
                        base = Hash::from(base_bytes);
                        continue;
                    }

                    return Ok(None);
                }
            },
            None => Ok(None),
        }
    }
}

fn snapshot_dir<P: AsRef<Path>>(dir: P, snap_hex: &str) -> PathBuf {
    dir.as_ref().join(snap_hex)
}

fn snapshot_base_path<P: AsRef<Path>>(dir: P, snap_hex: &str) -> PathBuf {
    snapshot_dir(dir, snap_hex).join("base")
}

fn module_path<P: AsRef<Path>>(
    dir: P,
    snap_hex: &str,
    module_id_hex: &str,
) -> PathBuf {
    snapshot_dir(dir, snap_hex).join(module_id_hex)
}

/// A copy-on-write or anonymous mmap that is a WASM linear memory.
#[derive(Debug)]
pub struct Memory {
    mmap: MmapMut,
    ptr: MmapPtr,
}

/// This allows `wasmer_vm::LinearMemory::vmmemory` to be implemented at the
/// cost of a small overhead of two pointer lengths.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct MmapPtr {
    base: *const u8,
    len: usize,
}

// this is safe because it always points to the base of the mmap, rather than to
// the `Memory` struct itself.
unsafe impl Send for MmapPtr {}
unsafe impl Sync for MmapPtr {}

impl<'a> From<&'a MmapMut> for MmapPtr {
    fn from(mmap: &'a MmapMut) -> Self {
        Self {
            base: mmap.as_ptr(),
            len: mmap.len(),
        }
    }
}

impl Memory {
    /// Creates a new copy-on-write WASM linear memory backed by a file at the
    /// given `path`.
    ///
    /// This will create the file if it doesn't exist. If the file is smaller
    /// than a WASM page it will extended and its contents zeroed.
    pub fn new<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(path)?;

        if file.metadata()?.len() < PAGE_SIZE as u64 {
            file.set_len(PAGE_SIZE as u64)?;
            file.write_all(&ZEROED_PAGE)?;
        }

        let mmap = unsafe { MmapOptions::new().map_copy(&file)? };
        let ptr = MmapPtr::from(&mmap);

        Ok(Self { mmap, ptr })
    }

    /// Creates a new anonymous WASM linear memory with an initial size of a
    /// WASM page.
    pub fn ephemeral() -> io::Result<Self> {
        let mmap = MmapMut::map_anon(PAGE_SIZE)?;
        let ptr = MmapPtr::from(&mmap);
        Ok(Self { mmap, ptr })
    }

    /// Copies the current contents onto the file at the given `path`, replacing
    /// the internal mmap by a new copy-on-write WASM backed by said file.
    ///
    /// The file will be truncated if it exists.
    pub fn copy_to<P: AsRef<Path>>(&mut self, path: P) -> io::Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(true)
            .open(path)?;

        file.set_len(self.mmap.len() as u64)?;
        file.write_all(&self.mmap)?;

        let new_mmap = unsafe { MmapOptions::new().map_copy(&file)? };
        let new_ptr = MmapPtr::from(&new_mmap);

        self.mmap = new_mmap;
        self.ptr = new_ptr;

        Ok(())
    }

    /// Grows the underlying mmap by creating a new anonymous mmap and copying
    /// the current contents into it.
    pub fn grow(&mut self, pages: usize) -> io::Result<()> {
        let curr_len = self.mmap.len();
        let new_len = curr_len + pages * PAGE_SIZE;

        let mut new_mmap = MmapMut::map_anon(new_len)?;
        let new_ptr = MmapPtr::from(&new_mmap);

        new_mmap[..curr_len].copy_from_slice(&self.mmap);

        self.mmap = new_mmap;
        self.ptr = new_ptr;

        Ok(())
    }
}

impl LinearMemory for Memory {
    fn ty(&self) -> MemoryType {
        MemoryType::new(1, None, true)
    }

    fn size(&self) -> Pages {
        Pages((self.mmap.len() / WASM_PAGE_SIZE) as u32)
    }

    fn style(&self) -> MemoryStyle {
        MemoryStyle::Dynamic {
            offset_guard_size: 0,
        }
    }

    fn grow(&mut self, delta: Pages) -> Result<Pages, MemoryError> {
        self.grow(delta.0 as usize)
            .map(|_| Pages((self.mmap.len() / WASM_PAGE_SIZE) as u32))
            .map_err(|err| MemoryError::Generic(format!("{}", err)))
    }

    fn vmmemory(&self) -> NonNull<VMMemoryDefinition> {
        let ptr = &self.ptr as *const MmapPtr;
        let ptr = ptr as *mut VMMemoryDefinition;
        NonNull::new(ptr).unwrap()
    }

    fn try_clone(&self) -> Option<Box<dyn LinearMemory + 'static>> {
        // TODO this could actually be implemented
        None
    }
}

impl Deref for Memory {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.mmap
    }
}

impl DerefMut for Memory {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.mmap
    }
}

impl AsRef<[u8]> for Memory {
    fn as_ref(&self) -> &[u8] {
        &self.mmap
    }
}

impl AsMut<[u8]> for Memory {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.mmap
    }
}

#[cfg(test)]
mod tests {
    use super::Memory;

    use crate::hash::Hash;
    use crate::session::MemorySession;

    use dallo::ModuleId;
    use rand::rngs::StdRng;
    use rand::{RngCore, SeedableRng};
    use tempfile::NamedTempFile;

    #[test]
    fn create_grow_copy() {
        let initial_file = NamedTempFile::new()
            .expect("tempfile creation should be successful");
        let after_file = NamedTempFile::new()
            .expect("tempfile creation should be successful");

        let mut mem = Memory::new(&initial_file)
            .expect("memory creation should be successful");

        // modify some memory
        mem[4] = 42;
        mem[13] = 7;

        // grow the memory by one page
        mem.grow(10).expect("growing should be successful");
        mem.copy_to(after_file)
            .expect("memory creation should be successful");

        assert_eq!(mem[4], 42, "new memory should have been changed");
        assert_eq!(mem[13], 7, "new memory should have been changed");

        // old memory should be untouched
        let old_mem = Memory::new(initial_file)
            .expect("memory creation should be successful");

        assert_eq!(old_mem[4], 0, "old memory should be unchanged");
        assert_eq!(old_mem[13], 0, "old memory should be unchanged");
    }

    #[test]
    fn snapping() {
        let memories_dir =
            tempfile::tempdir().expect("creating tmp dir should be fine");

        let mut session_1 = MemorySession::new(&memories_dir, Hash::ZERO)
            .expect("session creation should work");

        let module_1 = {
            let mut bytes = [0u8; 32];
            bytes[0] = 42;
            ModuleId::from(bytes)
        };

        let module_2 = {
            let mut bytes = [0u8; 32];
            bytes[1] = 42;
            ModuleId::from(bytes)
        };

        session_1.load(module_1).expect("loading should go ok");
        session_1.load(module_2).expect("loading should go ok");

        let mut mem_1 = session_1
            .borrow_mut(&module_1)
            .expect("borrowing should succeed");
        let mut mem_2 = session_1
            .borrow_mut(&module_2)
            .expect("borrowing should succeed");

        let mut rng = StdRng::seed_from_u64(1234);

        rng.fill_bytes(&mut mem_1[..]);
        rng.fill_bytes(&mut mem_2[..]);

        drop(mem_1);
        drop(mem_2);

        let snap = session_1.snap().expect("snap id");
        let root = session_1.root();
        assert_eq!(snap, root);

        let session_2 = MemorySession::new(&memories_dir, snap)
            .expect("session creation should work");
        let root = session_2.root();
        assert_eq!(snap, root);
    }

    #[cfg(feature = "test")]
    mod bench {
        extern crate test;

        use crate::hash::Hash;
        use crate::session::MemorySession;

        use dallo::ModuleId;
        use rand::rngs::StdRng;
        use rand::{RngCore, SeedableRng};
        use tempfile::TempDir;

        use test::Bencher;

        // the return needs to include TempDir, since if it drops it will remove
        // the directory with all its contents
        fn session(mem_num: usize) -> (MemorySession, TempDir) {
            let session_dir =
                tempfile::tempdir().expect("creating tmp dir should be fine");

            let mut session = MemorySession::new(&session_dir, Hash::ZERO)
                .expect("session creation should work");

            let mut rng = StdRng::seed_from_u64(42);

            let mut module_id = ModuleId::from([0u8; 32]);
            for _ in 0..mem_num {
                // this just adds one to the module ID
                module_id.as_mut().iter_mut().fold(
                    (1, true),
                    |(rhs, carry), b| {
                        let (new_b, carry) = b.carrying_add(rhs, carry);
                        *b = new_b;
                        (0, carry)
                    },
                );

                session.load(module_id).expect("loading should go ok");
                let mut mem = session.borrow_mut(&module_id).unwrap();
                rng.fill_bytes(&mut mem);
            }

            (session, session_dir)
        }

        #[bench]
        fn root_with_2_dirty_memories(bencher: &mut Bencher) {
            let (session, _dir) = session(2);
            bencher.iter(|| session.root())
        }

        #[bench]
        fn root_with_10_dirty_memories(bencher: &mut Bencher) {
            let (session, _dir) = session(10);
            bencher.iter(|| session.root())
        }

        #[bench]
        fn root_with_100_dirty_memories(bencher: &mut Bencher) {
            let (session, _dir) = session(100);
            bencher.iter(|| session.root())
        }

        #[bench]
        fn root_with_1000_dirty_memories(bencher: &mut Bencher) {
            let (session, _dir) = session(1000);
            bencher.iter(|| session.root())
        }
    }
}
