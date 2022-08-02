// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::cell::UnsafeCell;
use std::sync::Arc;

use wasmer::WasmerEnv;

use crate::instance::Instance;

#[derive(Debug)]
enum EnvInner {
    Uninitialized,
    Initialized(Instance),
}

#[derive(Clone, WasmerEnv, Debug)]
pub struct Env(Arc<UnsafeCell<EnvInner>>);

unsafe impl Sync for Env {}
unsafe impl Send for Env {}

impl Env {
    pub(crate) fn initialize(&mut self, instance: Instance) {
        unsafe {
            *self.0.get() = EnvInner::Initialized(instance);
        }
    }

    pub(crate) fn uninitialized() -> Self {
        Env(Arc::new(UnsafeCell::new(EnvInner::Uninitialized)))
    }

    pub(crate) fn inner(&self) -> &Instance {
        if let EnvInner::Initialized(ei) = unsafe { &*self.0.get() } {
            &ei
        } else {
            unreachable!("uninitialized env")
        }
    }

    pub(crate) fn inner_mut(&self) -> &mut Instance {
        if let EnvInner::Initialized(ref mut ei) = unsafe { &mut *self.0.get() }
        {
            ei
        } else {
            unreachable!("uninitialized env")
        }
    }
}
