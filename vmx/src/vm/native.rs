// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::collections::BTreeMap;
use std::fmt::{self, Debug, Formatter};

#[derive(Default)]
pub struct NativeQueries {
    map: BTreeMap<&'static str, Box<dyn NativeQuery>>,
}

impl Debug for NativeQueries {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.map.keys()).finish()
    }
}

impl NativeQueries {
    pub fn new() -> Self {
        NativeQueries {
            map: BTreeMap::new(),
        }
    }

    pub fn insert<Q>(&mut self, name: &'static str, query: Q)
    where
        Q: 'static + NativeQuery,
    {
        self.map.insert(name, Box::new(query));
    }

    pub fn call(&self, name: &str, buf: &mut [u8], len: u32) -> Option<u32> {
        self.map.get(name).map(|host_query| host_query(buf, len))
    }
}

/// A query executable on the host.
///
/// The buffer containing the argument the module used to call the query
/// together with its length are passed as arguments to the function, and should
/// be processed first. Once this is done, the implementor should emplace the
/// return of the query in the same buffer, and return its length.
pub trait NativeQuery: Fn(&mut [u8], u32) -> u32 + Send + Sync {}
impl<F> NativeQuery for F where F: Fn(&mut [u8], u32) -> u32 + Send + Sync {}
