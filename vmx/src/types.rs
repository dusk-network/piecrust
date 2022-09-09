// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::convert::Infallible;

use dallo::{ModuleId, SCRATCH_BUF_BYTES};
use rkyv::ser::serializers::{
    BufferScratch, BufferSerializer, BufferSerializerError,
    CompositeSerializer, CompositeSerializerError, FixedSizeScratchError,
};

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

pub type StandardBufSerializer<'a> = CompositeSerializer<
    BufferSerializer<&'a mut [u8]>,
    BufferScratch<&'a mut [u8; SCRATCH_BUF_BYTES]>,
>;

type SerializerError = CompositeSerializerError<
    BufferSerializerError,
    FixedSizeScratchError,
    Infallible,
>;

#[derive(Debug)]
pub enum Error {
    WasmerCompile(wasmer::CompileError),
    WasmerDeserialize(wasmer::DeserializeError),
    WasmerExport(wasmer::ExportError),
    WasmerInstantiation(wasmer::InstantiationError),
    WasmerRuntime(wasmer::RuntimeError),
    WasmerSerialize(wasmer::SerializeError),
    RkyvCheck,
    RkyvSerializer(SerializerError),
    OutOfPoints(ModuleId),
}

impl From<wasmer::CompileError> for Error {
    fn from(ce: wasmer::CompileError) -> Self {
        Self::WasmerCompile(ce)
    }
}

impl From<wasmer::DeserializeError> for Error {
    fn from(de: wasmer::DeserializeError) -> Self {
        Self::WasmerDeserialize(de)
    }
}

impl From<wasmer::ExportError> for Error {
    fn from(ee: wasmer::ExportError) -> Self {
        Self::WasmerExport(ee)
    }
}

impl From<wasmer::InstantiationError> for Error {
    fn from(ie: wasmer::InstantiationError) -> Self {
        Self::WasmerInstantiation(ie)
    }
}

impl From<wasmer::RuntimeError> for Error {
    fn from(re: wasmer::RuntimeError) -> Self {
        Self::WasmerRuntime(re)
    }
}

impl From<wasmer::SerializeError> for Error {
    fn from(se: wasmer::SerializeError) -> Self {
        Self::WasmerSerialize(se)
    }
}

impl From<SerializerError> for Error {
    fn from(se: SerializerError) -> Self {
        Self::RkyvSerializer(se)
    }
}
