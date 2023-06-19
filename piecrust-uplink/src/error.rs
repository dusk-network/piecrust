// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use bytecheck::CheckBytes;
use rkyv::{Archive, Deserialize, Serialize};

use core::fmt::{Display, Formatter};

/// The error possibly returned on an inter-contract-call.
//
// We do **not use rkyv** to pass it to the contract from the VM. Instead, we
// use use the calling convention being able to pass negative numbers to signal
// a failure.
//
// The contract writer, however, is free to pass it around and react to it if it
// wishes.
#[derive(Debug, Clone, Copy, Archive, Serialize, Deserialize)]
#[archive_attr(derive(CheckBytes))]
pub enum ContractError {
    PANIC,
    OUTOFGAS,
    OTHER(i32),
}

impl ContractError {
    /// Returns a contract error from a return `code`.
    ///
    /// # Panic
    /// Panics if the value is larger than or equal to 0.
    pub fn from_code(code: i32) -> Self {
        if code >= 0 {
            panic!(
                "A `ContractError` is never equal or larger than 0, got {code}"
            );
        }

        match code {
            -1 => Self::PANIC,
            -2 => Self::OUTOFGAS,
            v => Self::OTHER(v),
        }
    }
}

impl From<ContractError> for i32 {
    fn from(err: ContractError) -> Self {
        match err {
            ContractError::PANIC => -1,
            ContractError::OUTOFGAS => -2,
            ContractError::OTHER(c) => c,
        }
    }
}

impl Display for ContractError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            ContractError::PANIC => write!(f, "CONTRACT PANIC"),
            ContractError::OUTOFGAS => write!(f, "OUT OF GAS"),
            ContractError::OTHER(c) => write!(f, "OTHER: {c}"),
        }
    }
}
