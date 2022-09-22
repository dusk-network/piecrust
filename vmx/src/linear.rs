// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::ptr;
use std::ptr::NonNull;
use std::sync::Arc;

use parking_lot::RwLock;

use wasmer::{MemoryType, Pages};
use wasmer_vm::{
    LinearMemory, MemoryError, MemoryStyle, VMMemory, VMMemoryDefinition,
};

use crate::error::Error;
use crate::module::VolatileMem;
use crate::types::MemoryFreshness;
use crate::Error::{MemorySetupError, RegionError};

pub const MEMORY_PAGES: usize = 19;
pub const WASM_PAGE_SIZE: usize = 64 * 1024;
pub const MAX_MEMORY_PAGES: usize = (u32::MAX / WASM_PAGE_SIZE as u32) as usize;
pub const WASM_PAGE_LOG2: u32 = 16;

#[derive(Debug)]
struct LinearInner {
    file_opt: Option<File>,
    // Workaround for not overwriting memory on initialization,
    volatile: Vec<VolatileMem>,
    vol_buffer: Vec<u8>,
    fresh: MemoryFreshness,
    pub memory_definition: Option<VMMemoryDefinition>,
}

#[derive(Debug, Clone)]
pub struct Linear(Arc<RwLock<LinearInner>>);

impl From<Linear> for VMMemory {
    fn from(lin: Linear) -> VMMemory {
        VMMemory(Box::new(lin))
    }
}

impl Linear {
    /// Creates a new copy-on-write WASM linear memory backed by a file at the
    /// given `path`.
    pub fn new<P: AsRef<Path>>(
        path: Option<P>,
        accessible_size: usize,
        mapping_size: usize,
        fresh: MemoryFreshness,
        volatile: Vec<VolatileMem>,
    ) -> Result<Self, Error>
    where
        P: std::fmt::Debug,
    {
        let mut ret = Linear(Arc::new(RwLock::new(LinearInner {
            file_opt: None,
            memory_definition: None,
            vol_buffer: vec![],
            fresh,
            volatile,
        })));

        let ptr: *mut std::ffi::c_void;
        let file_opt = match path {
            Some(file_path) => {
                if let Some(p) = file_path.as_ref().parent() {
                    std::fs::create_dir_all(p).map_err(MemorySetupError)?;
                }
                let file_path_exists = file_path.as_ref().exists();
                let file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(!file_path_exists)
                    .open(file_path)
                    .map_err(MemorySetupError)?;
                if !file_path_exists {
                    file.set_len(accessible_size as u64)
                        .map_err(MemorySetupError)?;
                };
                ptr = unsafe {
                    libc::mmap(
                        ptr::null_mut(),
                        mapping_size,
                        libc::PROT_NONE,
                        libc::MAP_SHARED,
                        file.as_raw_fd(),
                        0,
                    )
                };
                Some(file)
            }
            None => {
                ptr = unsafe {
                    libc::mmap(
                        ptr::null_mut(),
                        mapping_size,
                        libc::PROT_NONE,
                        libc::MAP_PRIVATE | libc::MAP_ANON,
                        -1,
                        0,
                    )
                };
                None
            }
        };
        if ptr as isize == -1_isize {
            return Err(MemorySetupError(io::Error::last_os_error()));
        }

        {
            let mut guard = ret.0.write();

            let LinearInner {
                ref mut memory_definition,
                ..
            } = *guard;

            *memory_definition = Some(VMMemoryDefinition {
                base: ptr as _,
                current_length: accessible_size,
            });

            guard.file_opt = file_opt;
        }

        if accessible_size != 0 {
            // Commit the accessible size.
            ret.make_accessible(0, accessible_size)?
        }

        Ok(ret)
    }

    /// Make the memory starting at `start` and extending for `len` bytes
    /// accessible. `start` and `len` must be native page-size multiples and
    /// describe a range within `self`'s reserved memory.
    #[cfg(not(target_os = "windows"))]
    pub fn make_accessible(
        &mut self,
        start: usize,
        len: usize,
    ) -> Result<(), Error> {
        let page_size = region::page::size();
        assert_eq!(start & (page_size - 1), 0);
        assert_eq!(len & (page_size - 1), 0);

        if let Some(file) = &self.0.read().file_opt {
            if start > 0 {
                let new_len = (start + len) as u64;
                file.set_len(new_len).map_err(MemorySetupError)?;
            }
        }
        // Commit the accessible size.
        let guard = self.0.read();
        let vm_def_ptr = guard.memory_definition.as_ref().unwrap(); //.base as *const u8;
        let ptr = vm_def_ptr.base;
        unsafe {
            region::protect(ptr.add(start), len, region::Protection::READ_WRITE)
        }
        .map_err(RegionError)?;
        Ok(())
    }

    pub fn definition(&self) -> VMMemoryDefinition {
        self.0.read().memory_definition.unwrap()
    }

    pub fn definition_ptr(&self) -> NonNull<VMMemoryDefinition> {
        let r = &mut self.0.write().memory_definition.unwrap();
        NonNull::new(r).unwrap()
    }

    // workaround, to be deprecatedd
    pub(crate) fn save_volatile(&self) {
        let base: *mut u8;
        {
            let imm_guard = self.0.read();
            base = imm_guard.memory_definition.unwrap().base;
        }
        let mut guard = self.0.write();
        let inner = &mut *guard;

        inner.vol_buffer.truncate(0);
        for reg in &inner.volatile {
            let slice = unsafe {
                std::slice::from_raw_parts(base.add(reg.offset), reg.length)
            };
            inner.vol_buffer.extend_from_slice(slice);
        }
    }

    // workaround, to be deprecated
    pub(crate) fn restore_volatile(&self) {
        let base: *mut u8;
        {
            let imm_guard = self.0.read();
            base = imm_guard.memory_definition.unwrap().base;
        }
        let mut guard = self.0.write();
        let inner = &mut *guard;
        let mut buf_ofs = 0;

        for reg in &inner.volatile {
            unsafe {
                std::slice::from_raw_parts_mut(base.add(reg.offset), reg.length)
            }
            .copy_from_slice(&inner.vol_buffer[buf_ofs..][..reg.length]);
            buf_ofs += reg.length;
        }
    }

    // workaround, to be deprecated
    pub fn freshness(&self) -> MemoryFreshness {
        let guard = self.0.read();
        let inner = &*guard;

        inner.fresh
    }

    // workaround, to be deprecated
    pub fn set_freshness(&self, to: MemoryFreshness) {
        let mut guard = self.0.write();
        let inner = &mut *guard;

        inner.fresh = to
    }
}

impl LinearMemory for Linear {
    fn ty(&self) -> MemoryType {
        MemoryType {
            minimum: Pages::from(MEMORY_PAGES as u32),
            maximum: Some(Pages::from(MEMORY_PAGES as u32)),
            shared: false,
        }
    }

    fn size(&self) -> Pages {
        Pages::from(MEMORY_PAGES as u32)
    }

    fn style(&self) -> MemoryStyle {
        MemoryStyle::Static {
            bound: Pages::from(MEMORY_PAGES as u32),
            offset_guard_size: 0,
        }
    }

    fn grow(&mut self, delta: Pages) -> Result<Pages, MemoryError> {
        println!("grow begin delta={:?}", delta);
        let prev_pages = Pages::from(
            self.definition().current_length as u32 >> WASM_PAGE_LOG2,
        );

        if delta.0 == 0 {
            return Ok(prev_pages);
        }

        let new_pages = prev_pages + delta;
        let delta_bytes = delta.bytes().0;
        let prev_bytes = prev_pages.bytes().0;

        self.make_accessible(prev_bytes, delta_bytes)
            .map_err(|_e| {
                MemoryError::Region("todo error to string conv".to_string())
            })?;

        unsafe {
            let mut md_ptr = self.definition_ptr();
            let md = md_ptr.as_mut();
            md.current_length = new_pages.bytes().0;
        }

        Ok(prev_pages)
    }

    fn vmmemory(&self) -> NonNull<VMMemoryDefinition> {
        self.definition_ptr()
    }

    fn try_clone(&self) -> Option<Box<dyn LinearMemory + 'static>> {
        None
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use tempfile::tempdir;

    use crate::instance::InstanceTunables;
    use crate::types::MemoryFreshness::*;
    use wasmer::{
        imports, wat2wasm, Instance, Memory, Module, Store, TypedFunction,
    };
    use wasmer_compiler_singlepass::Singlepass;

    #[test]
    fn instantiate_test() -> Result<(), Error> {
        let wasm_bytes = wat2wasm(
            br#"(module
            (memory (;0;) 18)
            (global (;0;) (mut i32) i32.const 1048576)
            (export "memory" (memory 0))
            (data (;0;) (i32.const 1048576) "*\00\00\00")
          )"#,
        )
        .unwrap();
        let compiler = Singlepass::default();

        let tunables = InstanceTunables::new(Linear::new(
            Some(
                tempdir()
                    .map_err(MemorySetupError)?
                    .path()
                    .join("instantiate_test"),
            ),
            MEMORY_PAGES * WASM_PAGE_SIZE,
            MEMORY_PAGES * WASM_PAGE_SIZE,
            Fresh,
            vec![],
        )?);
        let mut store = Store::new_with_tunables(compiler, tunables);
        let module = Module::new(&store, wasm_bytes).unwrap();
        let import_object = imports! {};
        let instance =
            Instance::new(&mut store, &module, &import_object).unwrap();

        let mut memories: Vec<Memory> = instance
            .exports
            .iter()
            .memories()
            .map(|pair| pair.1.clone())
            .collect();
        assert_eq!(memories.len(), 1);
        let first_memory = memories.pop().unwrap();
        assert_eq!(
            first_memory.ty(&store).maximum.unwrap(),
            Pages(MEMORY_PAGES as u32)
        );
        let view = first_memory.view(&store);

        let x = unsafe { view.data_unchecked_mut() }[0];
        assert_eq!(x, 0);
        Ok(())
    }

    #[test]
    fn micro_test() -> Result<(), Error> {
        let wasm_bytes = module_bytecode!("micro");

        let compiler = Singlepass::default();

        let tunables = InstanceTunables::new(Linear::new(
            Some(
                tempdir()
                    .map_err(MemorySetupError)?
                    .path()
                    .join("micro_test"),
            ),
            MEMORY_PAGES * WASM_PAGE_SIZE,
            MEMORY_PAGES * WASM_PAGE_SIZE,
            Fresh,
            vec![],
        )?);
        let mut store = Store::new_with_tunables(compiler, tunables);
        let module = Module::new(&store, wasm_bytes).unwrap();
        let import_object = imports! {};
        let instance =
            Instance::new(&mut store, &module, &import_object).unwrap();

        let fun: TypedFunction<u32, u32> = instance
            .exports
            .get_typed_function(&store, "change")
            .unwrap();

        assert_eq!(fun.call(&mut store, 43).unwrap(), 42);
        assert_eq!(fun.call(&mut store, 44).unwrap(), 43);
        Ok(())
    }
}
