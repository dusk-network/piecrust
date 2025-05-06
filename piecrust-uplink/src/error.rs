// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

extern crate alloc;

use bytecheck::CheckBytes;
use rkyv::{Archive, Deserialize, Serialize};

use alloc::string::String;

use core::fmt::{Display, Formatter};
use core::str;

/// The error possibly returned on an inter-contract-call.
//
// We do **not use rkyv** to pass it to the contract from the VM. Instead, we
// use use the calling convention being able to pass negative numbers to signal
// a failure.
//
// The contract writer, however, is free to pass it around and react to it if it
// wishes.
#[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize)]
#[archive_attr(derive(CheckBytes))]
pub enum ContractError {
    Panic(String),
    OutOfGas,
    DoesNotExist,
    Unknown,
}

impl ContractError {
    /// Returns a contract error from a return `code` and the data in the
    /// `slice`.
    #[cfg(feature = "abi")]
    pub(crate) fn from_parts(code: i32, slice: &[u8]) -> Self {
        fn get_msg(slice: &[u8]) -> String {
            let msg_len = {
                let mut msg_len_bytes = [0u8; 4];
                msg_len_bytes.copy_from_slice(&slice[..4]);
                u32::from_le_bytes(msg_len_bytes)
            } as usize;

            // SAFETY: the host guarantees that the message is valid UTF-8,
            // so this is safe.
            let msg = unsafe {
                use alloc::string::ToString;
                let msg_bytes = &slice[4..4 + msg_len];
                let msg_str = str::from_utf8_unchecked(msg_bytes);
                msg_str.to_string()
            };

            msg
        }

        match code {
            -1 => Self::Panic(get_msg(slice)),
            -2 => Self::OutOfGas,
            -3 => Self::DoesNotExist,
            i32::MIN => Self::Unknown,
            _ => unreachable!("The host must guarantee that the code is valid"),
        }
    }

    /// Write the appropriate data the `arg_buf` and return the error code.
    pub fn to_parts(&self, slice: &mut [u8]) -> i32 {
        fn put_msg(msg: &str, slice: &mut [u8]) {
            let msg_bytes = msg.as_bytes();
            let msg_len = msg_bytes.len();

            let mut msg_len_bytes = [0u8; 4];
            msg_len_bytes.copy_from_slice(&(msg_len as u32).to_le_bytes());

            slice[..4].copy_from_slice(&msg_len_bytes);
            slice[4..4 + msg_len].copy_from_slice(msg_bytes);
        }

        match self {
            Self::Panic(msg) => {
                put_msg(msg, slice);
                -1
            }
            Self::OutOfGas => -2,
            Self::DoesNotExist => -3,
            Self::Unknown => i32::MIN,
        }
    }
}

impl From<ContractError> for i32 {
    fn from(err: ContractError) -> Self {
        match err {
            ContractError::Panic(_) => -1,
            ContractError::OutOfGas => -2,
            ContractError::DoesNotExist => -3,
            ContractError::Unknown => i32::MIN,
        }
    }
}

impl Display for ContractError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            ContractError::Panic(msg) => write!(f, "Panic: {msg}"),
            ContractError::OutOfGas => write!(f, "OutOfGas"),
            ContractError::DoesNotExist => {
                write!(f, "Contract does not exist")
            }
            ContractError::Unknown => write!(f, "Unknown"),
        }
    }
}
