// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use alloc::string::String;
use alloc::vec::Vec;
use core::ptr;

use bytecheck::CheckBytes;
use rkyv::{
    ser::serializers::{BufferScratch, BufferSerializer, CompositeSerializer},
    Archive, Deserialize, Serialize,
};

use crate::{ECO_MODE_LEN, SCRATCH_BUF_BYTES};
use EconomicMode::*;

/// And event emitted by a contract.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Archive,
    Serialize,
    Deserialize,
)]
#[archive_attr(derive(CheckBytes))]
pub struct Event {
    pub source: ContractId,
    pub topic: String,
    pub data: Vec<u8>,
}

/// Type with `rkyv` serialization capabilities for specific types.
pub type StandardBufSerializer<'a> = CompositeSerializer<
    BufferSerializer<&'a mut [u8]>,
    BufferScratch<&'a mut [u8; SCRATCH_BUF_BYTES]>,
>;

/// The length of [`ContractId`] in bytes
pub const CONTRACT_ID_BYTES: usize = 32;

/// ID to identify the wasm contracts after they have been deployed
#[derive(
    PartialEq,
    Eq,
    Archive,
    Serialize,
    CheckBytes,
    Deserialize,
    PartialOrd,
    Ord,
    Hash,
    Clone,
    Copy,
)]
#[archive(as = "Self")]
#[repr(C)]
pub struct ContractId([u8; CONTRACT_ID_BYTES]);

impl ContractId {
    /// Creates a placeholder [`ContractId`] until the host deploys the contract
    /// and sets a real [`ContractId`]. This can also be used to determine if a
    /// contract is the first to be called.
    pub const fn uninitialized() -> Self {
        ContractId([0u8; CONTRACT_ID_BYTES])
    }

    /// Creates a new [`ContractId`] from an array of bytes
    pub const fn from_bytes(bytes: [u8; CONTRACT_ID_BYTES]) -> Self {
        Self(bytes)
    }

    /// Returns the array of bytes that make up the [`ContractId`]
    pub const fn to_bytes(self) -> [u8; CONTRACT_ID_BYTES] {
        self.0
    }

    /// Returns a reference to the array of bytes that make up the
    /// [`ContractId`]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Returns a mutable reference to the array of bytes that make up the
    /// [`ContractId`]
    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }

    /// Determines whether the [`ContractId`] is uninitialized, which can be
    /// used to check if this contract is the first to be called.
    pub fn is_uninitialized(&self) -> bool {
        self == &Self::uninitialized()
    }
}

impl From<[u8; CONTRACT_ID_BYTES]> for ContractId {
    fn from(bytes: [u8; CONTRACT_ID_BYTES]) -> Self {
        Self::from_bytes(bytes)
    }
}

impl AsRef<[u8]> for ContractId {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl AsMut<[u8]> for ContractId {
    fn as_mut(&mut self) -> &mut [u8] {
        self.as_bytes_mut()
    }
}

impl core::fmt::Debug for ContractId {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        core::fmt::Display::fmt(self, f)
    }
}

impl core::fmt::Display for ContractId {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        if f.alternate() {
            write!(f, "0x")?
        }
        for byte in self.0 {
            write!(f, "{:02x}", &byte)?
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
/// Result of the `raw` contract calls.
pub struct RawResult {
    pub data: Vec<u8>,
    pub economic_mode: EconomicMode,
}

impl RawResult {
    pub fn new(data: Vec<u8>, eco_mode: EconomicMode) -> Self {
        Self {
            data,
            economic_mode: eco_mode,
        }
    }
    pub fn empty() -> Self {
        Self {
            data: Vec::new(),
            economic_mode: EconomicMode::None,
        }
    }
}

#[derive(Debug, Clone, Archive, Deserialize, Serialize, Eq, PartialEq)]
#[archive_attr(derive(CheckBytes))]
pub enum EconomicMode {
    Allowance(u64),
    None,
    Unknown,
}

impl EconomicMode {
    const ALLOWANCE: u8 = 1;
    const NONE: u8 = 0;

    pub fn write(&self, buf: &mut [u8]) {
        fn write_value(value: &u64, buf: &mut [u8]) {
            let slice = &mut buf[1..ECO_MODE_LEN];
            slice.copy_from_slice(&value.to_le_bytes()[..]);
        }
        fn zero_buf(buf: &mut [u8]) {
            unsafe { ptr::write_bytes(buf.as_mut_ptr(), 0u8, ECO_MODE_LEN) }
        }
        match self {
            Allowance(allowance) => {
                write_value(allowance, buf);
                buf[0] = Self::ALLOWANCE;
            }
            None => {
                zero_buf(buf);
                buf[0] = Self::NONE;
            }
            _ => panic!("Incorrect economic mode"),
        }
    }

    pub fn read(buf: &[u8]) -> Self {
        let value = || {
            let mut value_bytes = [0; 8];
            value_bytes.copy_from_slice(&buf[1..ECO_MODE_LEN]);
            u64::from_le_bytes(value_bytes)
        };
        match buf[0] {
            Self::ALLOWANCE => Allowance(value()),
            Self::NONE => None,
            _ => Unknown,
        }
    }
}
