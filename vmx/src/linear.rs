// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::cell::UnsafeCell;
use std::ptr::NonNull;

pub const MEMORY_PAGES: usize = 18;
const WASM_PAGE_SIZE: usize = 64 * 1024;
//const MEMORY_BYTES: usize = MEMORY_PAGES * WASM_PAGE_SIZE;

use wasmer::{MemoryType, Pages};
use wasmer_vm::{
    LinearMemory, MaybeInstanceOwned, MemoryError, MemoryStyle, VMMemory,
    VMMemoryDefinition,
};

#[derive(Debug)]
pub struct Linear {
    mem: Vec<u8>,
    pub memory_definition: Option<UnsafeCell<VMMemoryDefinition>>,
}

unsafe impl Send for Linear {}
unsafe impl Sync for Linear {}

// impl Clone for Linear {
//     fn clone(&self) -> Self {
//         todo!("what")
//     }
// }

impl Into<VMMemory> for Linear {
    fn into(self) -> VMMemory {
        VMMemory(Box::new(self))
    }
}

impl Linear {
    pub fn new() -> Self {
        let sz = 18 * WASM_PAGE_SIZE;
        let mut memory = Vec::new();
        memory.resize(sz, 0);
        let mut ret = Linear {
            mem: memory,
            memory_definition: None,
        };
        ret.memory_definition = Some(UnsafeCell::new(VMMemoryDefinition {
            base: ret.mem.as_ptr() as _,
            current_length: sz,
        }));
        ret
    }
}

impl LinearMemory for Linear {
    fn ty(&self) -> MemoryType {
        MemoryType {
            minimum: Pages::from(18u32),
            maximum: Some(Pages::from(18u32)),
            shared: false,
        }
    }

    fn size(&self) -> Pages {
        Pages::from(18u32)
    }

    fn style(&self) -> MemoryStyle {
        MemoryStyle::Static {
            bound: Pages::from(18u32),
            offset_guard_size: 0,
        }
    }

    fn grow(&mut self, delta: Pages) -> Result<Pages, MemoryError> {
        Err(MemoryError::CouldNotGrow {
            current: Pages::from(100u32),
            attempted_delta: delta,
        })
    }

    fn vmmemory(&self) -> NonNull<VMMemoryDefinition> {
        unsafe {
            NonNull::new(
                self.memory_definition
                    .as_ref()
                    .unwrap()
                    .get()
                    .as_mut()
                    .unwrap() as _,
            )
            .unwrap()
        }
    }

    fn try_clone(&self) -> Option<Box<dyn LinearMemory + 'static>> {
        None
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn instanciate_test() {
        use wasmer::{imports, wat2wasm, Instance, Memory, Module, Store};
        use wasmer_compiler_singlepass::Singlepass;

        use crate::session::SessionTunables;

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

        let tunables = SessionTunables::new();
        let mut store = Store::new_with_tunables(compiler, tunables);
        //let mut store = Store::new(compiler);
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
        assert_eq!(first_memory.ty(&store).maximum.unwrap(), Pages(18));
        let view = first_memory.view(&store);

        let x = unsafe { view.data_unchecked_mut() }[0];
        assert_eq!(x, 0);
    }
}
