// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::borrow::Cow;
use thiserror::Error;

use rkyv::ser::serializers::{
    BufferSerializerError, CompositeSerializerError, FixedSizeScratchError,
};

pub type Compo = CompositeSerializerError<
    BufferSerializerError,
    FixedSizeScratchError,
    std::convert::Infallible,
>;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    InstantiationError(#[from] wasmer::InstantiationError),
    #[error(transparent)]
    CompileError(#[from] wasmer::CompileError),
    #[error(transparent)]
    ExportError(#[from] wasmer::ExportError),
    #[error(transparent)]
    RuntimeError(#[from] wasmer::RuntimeError),
    #[error(transparent)]
    SerializeError(#[from] wasmer::SerializeError),
    #[error(transparent)]
    DeserializeError(#[from] wasmer::DeserializeError),
    #[error(transparent)]
    ParsingError(Box<wasmer::wasmparser::BinaryReaderError>),
    #[error("WASMER TRAP")]
    Trap(Box<wasmer_vm::Trap>),
    #[error(transparent)]
    CompositeSerializerError(#[from] Compo),
    #[error(transparent)]
    PersistenceError(std::io::Error),
    #[error(transparent)]
    CommitError(std::io::Error),
    #[error(transparent)]
    RestoreError(std::io::Error),
    #[error("Session error: {0}")]
    SessionError(Cow<'static, str>),
    #[error(transparent)]
    MemorySetupError(std::io::Error),
    #[error(transparent)]
    RegionError(region::Error),
    #[error("ValidationError")]
    ValidationError,
    #[error("OutOfPoints")]
    OutOfPoints,
}

impl From<wasmer_vm::Trap> for Error {
    fn from(e: wasmer_vm::Trap) -> Self {
        Error::Trap(Box::from(e))
    }
}

impl<A, B> From<rkyv::validation::CheckArchiveError<A, B>> for Error {
    fn from(_e: rkyv::validation::CheckArchiveError<A, B>) -> Self {
        Error::ValidationError
    }
}
