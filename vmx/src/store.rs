use parking_lot::RwLock;
use std::sync::Arc;

#[derive(Default)]
struct VersionedStoreInner {
    active: wasmer::Store,
}

#[derive(Clone, Default)]
pub struct VersionedStore(Arc<RwLock<VersionedStoreInner>>);

impl VersionedStore {
    pub fn inner<R, F>(&self, f: F) -> R
    where
        F: FnOnce(&wasmer::Store) -> R,
    {
        println!("inner");
        let r = f(&self.0.read().active);
        println!("inner fin");
        r
    }

    pub fn inner_mut<R, F>(&self, f: F) -> R
    where
        F: FnOnce(&mut wasmer::Store) -> R,
    {
        println!("inner_mut");
        let r = f(&mut self.0.write().active);
        println!("inner_mut fin");
        r
    }
}
