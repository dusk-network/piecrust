// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

// use std::fmt::{Display, Formatter};

use rkyv::ser::serializers::{
    BufferSerializerError, CompositeSerializerError, FixedSizeScratchError,
};

pub type Compo = CompositeSerializerError<
    BufferSerializerError,
    FixedSizeScratchError,
    std::convert::Infallible,
>;

#[derive(Debug)]
pub enum Error {
    InstantiationError(wasmer::InstantiationError),
    CompileError(Box<wasmer::CompileError>),
    ExportError(Box<wasmer::ExportError>),
    RuntimeError(wasmer::RuntimeError),
    SerializeError(Box<wasmer::SerializeError>),
    DeserializeError(Box<wasmer::DeserializeError>),
    ParsingError(Box<wasmparser::BinaryReaderError>),
    Trap(wasmer_vm::Trap),
    CompositeSerializerError(Compo),
    PersistenceError(std::io::Error),
    CommitError(std::io::Error),
    MemorySetupError(std::io::Error),
    RegionError(region::Error),
    ValidationError,
}

impl From<wasmer::InstantiationError> for Error {
    fn from(e: wasmer::InstantiationError) -> Self {
        Error::InstantiationError(e)
    }
}

impl From<wasmer::CompileError> for Error {
    fn from(e: wasmer::CompileError) -> Self {
        Error::CompileError(Box::from(e))
    }
}

impl From<wasmer::ExportError> for Error {
    fn from(e: wasmer::ExportError) -> Self {
        Error::ExportError(Box::from(e))
    }
}

impl From<wasmer::RuntimeError> for Error {
    fn from(e: wasmer::RuntimeError) -> Self {
        Error::RuntimeError(e)
    }
}

impl From<wasmer::SerializeError> for Error {
    fn from(e: wasmer::SerializeError) -> Self {
        Error::SerializeError(Box::from(e))
    }
}

impl From<wasmer::DeserializeError> for Error {
    fn from(e: wasmer::DeserializeError) -> Self {
        Error::DeserializeError(Box::from(e))
    }
}

impl From<Compo> for Error {
    fn from(e: Compo) -> Self {
        Error::CompositeSerializerError(e)
    }
}

impl From<wasmer_vm::Trap> for Error {
    fn from(e: wasmer_vm::Trap) -> Self {
        Error::Trap(e)
    }
}

impl<A, B> From<rkyv::validation::CheckArchiveError<A, B>> for Error {
    fn from(_e: rkyv::validation::CheckArchiveError<A, B>) -> Self {
        Error::ValidationError
    }
}
