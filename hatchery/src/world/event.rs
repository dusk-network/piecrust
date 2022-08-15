// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use dallo::ModuleId;
use std::ops::Deref;

/// The receipt of a query or transaction, containing the return and the events
/// emitted.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Receipt<T> {
    ret: T,
    events: Vec<Event>,
    spent: u64,
}

impl<T> Receipt<T> {
    pub(crate) fn new(ret: T, events: Vec<Event>, spent: u64) -> Self {
        Self { ret, events, spent }
    }

    /// Get the return of the query or transaction.
    pub fn ret(&self) -> &T {
        &self.ret
    }

    /// Return the events emitted.
    pub fn events(&self) -> &[Event] {
        &self.events
    }

    /// Return the points spent by the call.
    pub fn spent(&self) -> u64 {
        self.spent
    }

    /// Convert into result
    pub fn into_inner(self) -> T {
        self.ret
    }
}

impl<T> Deref for Receipt<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.ret()
    }
}

/// An event emitted by a module.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Event {
    module_id: ModuleId,
    data: Vec<u8>,
}

impl Event {
    pub(crate) fn new(module_id: ModuleId, data: Vec<u8>) -> Self {
        Self { module_id, data }
    }

    /// Return the id of the module that emitted this event.
    pub fn module_id(&self) -> &ModuleId {
        &self.module_id
    }

    /// Return data contained with the event
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}
