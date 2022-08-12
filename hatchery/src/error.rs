// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::io;

use dallo::ModuleId;
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
    Trap(wasmer_vm::Trap),
    MissingModuleExport,
    CompositeSerializerError(Compo),
    PersistenceError(io::Error),
    OutOfPoints(ModuleId),
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
