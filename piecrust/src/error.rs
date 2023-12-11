// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::borrow::Cow;
use std::sync::{mpsc, Arc};
use thiserror::Error;

use piecrust_uplink::{ContractError, ContractId};
use rkyv::ser::serializers::{
    BufferSerializerError, CompositeSerializerError, FixedSizeScratchError,
};

pub type Compo = CompositeSerializerError<
    BufferSerializerError,
    FixedSizeScratchError,
    std::convert::Infallible,
>;

/// The error type returned by the piecrust VM.
#[derive(Error, Debug)]
pub enum Error {
    #[error("Commit error: {0}")]
    CommitError(Cow<'static, str>),
    #[error(transparent)]
    CompositeSerializerError(Arc<Compo>),
    #[error(transparent)]
    ContractCacheError(Arc<std::io::Error>),
    #[error("Contract does not exist: {0}")]
    ContractDoesNotExist(ContractId),
    #[error(transparent)]
    FeedPulled(mpsc::SendError<Vec<u8>>),
    #[error(transparent)]
    Infallible(std::convert::Infallible),
    #[error("InitalizationError: {0}")]
    InitalizationError(Cow<'static, str>),
    #[error("Invalid global")]
    InvalidArgumentBuffer,
    #[error("Invalid function: {0}")]
    InvalidFunction(String),
    #[error("Invalid memory")]
    InvalidMemory,
    #[error("Memory access out of bounds: offset {offset}, length {len}, memory length {mem_len}")]
    MemoryAccessOutOfBounds {
        offset: usize,
        len: usize,
        mem_len: usize,
    },
    #[error("Snapshot failure: {reason:?} {io}")]
    MemorySnapshotFailure {
        reason: Option<Arc<Self>>,
        io: Arc<std::io::Error>,
    },
    #[error("Missing feed")]
    MissingFeed,
    #[error("Missing host data: {0}")]
    MissingHostData(String),
    #[error("Missing host query: {0}")]
    MissingHostQuery(String),
    #[error("OutOfPoints")]
    OutOfPoints,
    #[error("Panic: {0}")]
    Panic(String),
    #[error(transparent)]
    PersistenceError(Arc<std::io::Error>),
    #[error(transparent)]
    RestoreError(Arc<std::io::Error>),
    #[error(transparent)]
    RuntimeError(dusk_wasmtime::Error),
    #[error("Session error: {0}")]
    SessionError(Cow<'static, str>),
    #[error("Too many memories: {0}")]
    TooManyMemories(usize),
    #[error(transparent)]
    Utf8(std::str::Utf8Error),
    #[error("ValidationError")]
    ValidationError,
}

impl Error {
    pub fn normalize(self) -> Self {
        match self {
            Self::RuntimeError(rerr) => match rerr.downcast() {
                Ok(err) => err,
                Err(rerr) => Self::RuntimeError(rerr),
            },
            err => err,
        }
    }
}

impl From<std::convert::Infallible> for Error {
    fn from(err: std::convert::Infallible) -> Self {
        Self::Infallible(err)
    }
}

impl From<std::str::Utf8Error> for Error {
    fn from(err: std::str::Utf8Error) -> Self {
        Self::Utf8(err)
    }
}

impl From<dusk_wasmtime::Error> for Error {
    fn from(e: dusk_wasmtime::Error) -> Self {
        Error::RuntimeError(e)
    }
}

impl From<Compo> for Error {
    fn from(e: Compo) -> Self {
        Error::CompositeSerializerError(Arc::from(e))
    }
}

impl<A, B> From<rkyv::validation::CheckArchiveError<A, B>> for Error {
    fn from(_e: rkyv::validation::CheckArchiveError<A, B>) -> Self {
        Error::ValidationError
    }
}

impl From<Error> for ContractError {
    fn from(err: Error) -> Self {
        match err {
            Error::OutOfPoints => Self::OutOfPoints,
            Error::Panic(msg) => Self::Panic(msg),
            _ => Self::Unknown,
        }
    }
}
