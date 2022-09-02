use crate::store::VersionedStore;
use crate::types::Error;

pub struct WrappedModule {
    module: wasmer::Module,
    store: VersionedStore,
}

impl WrappedModule {
    pub fn new(bytecode: &[u8]) -> Result<Self, Error> {
        let versioned = VersionedStore::default();
        println!("wrapped module new");
        versioned.inner(|store| {
            let module = wasmer::Module::new(store, bytecode)?;
            Ok(WrappedModule {
                store: versioned.clone(),
                module,
            })
        })
    }

    pub fn store(&self) -> &VersionedStore {
        &self.store
    }

    pub fn inner(&self) -> &wasmer::Module {
        &self.module
    }
}
