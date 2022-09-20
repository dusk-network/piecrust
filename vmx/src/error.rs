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
    CompileError(wasmer::CompileError),
    ExportError(wasmer::ExportError),
    RuntimeError(wasmer::RuntimeError),
    SerializeError(wasmer::SerializeError),
    DeserializeError(wasmer::DeserializeError),
    ParsingError(wasmparser::BinaryReaderError),
    // Trap(wasmer_vm::Trap),
    CompositeSerializerError(Box<Compo>),
    PersistenceError(Box<std::io::Error>),
    CommitError(Box<std::io::Error>),
    MemorySetupError(Box<std::io::Error>),
    RegionError(Box<region::Error>),
    ValidationError,
}

impl From<wasmer::InstantiationError> for Error {
    fn from(e: wasmer::InstantiationError) -> Self {
        Error::InstantiationError(e)
    }
}

impl From<wasmer::CompileError> for Error {
    fn from(e: wasmer::CompileError) -> Self {
        Error::CompileError(e)
    }
}

impl From<wasmer::ExportError> for Error {
    fn from(e: wasmer::ExportError) -> Self {
        Error::ExportError(e)
    }
}

impl From<wasmer::RuntimeError> for Error {
    fn from(e: wasmer::RuntimeError) -> Self {
        Error::RuntimeError(e)
    }
}

impl From<wasmer::SerializeError> for Error {
    fn from(e: wasmer::SerializeError) -> Self {
        Error::SerializeError(e)
    }
}

impl From<wasmer::DeserializeError> for Error {
    fn from(e: wasmer::DeserializeError) -> Self {
        Error::DeserializeError(e)
    }
}

impl From<Compo> for Error {
    fn from(e: Compo) -> Self {
        Error::CompositeSerializerError(Box::from(e))
    }
}

// impl From<wasmer_vm::Trap> for Error {
//     fn from(e: wasmer_vm::Trap) -> Self {
//         Error::Trap(e)
//     }
// }

impl<A, B> From<rkyv::validation::CheckArchiveError<A, B>> for Error {
    fn from(_e: rkyv::validation::CheckArchiveError<A, B>) -> Self {
        Error::ValidationError
    }
}
