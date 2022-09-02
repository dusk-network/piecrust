// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

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
        f(&self.0.read().active)
    }

    pub fn inner_mut<R, F>(&self, f: F) -> R
    where
        F: FnOnce(&mut wasmer::Store) -> R,
    {
        f(&mut self.0.write().active)
    }
}
