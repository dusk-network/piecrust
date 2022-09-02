use crate::store::VersionedStore;
use crate::types::Error;

pub struct WrappedModule {
    serialized: Vec<u8>,
    store: VersionedStore,
}

impl WrappedModule {
    pub fn new(bytecode: &[u8]) -> Result<Self, Error> {
        let versioned = VersionedStore::default();
        versioned.inner(|store| {
            let module = wasmer::Module::new(store, bytecode)?;
            let serialized = module.serialize()?;

            Ok(WrappedModule {
                store: versioned.clone(),
                serialized,
            })
        })
    }

    pub fn store(&self) -> &VersionedStore {
        &self.store
    }

    pub fn module(&self) -> Result<wasmer::Module, Error> {
        self.store
            .inner(|store| unsafe {
                wasmer::Module::deserialize(store, &self.serialized)
            })
            .map_err(Into::into)
    }
}
