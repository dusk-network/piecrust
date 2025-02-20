// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use alloc::string::String;
use alloc::vec::Vec;

use bytecheck::CheckBytes;
use rkyv::{
    ser::serializers::{BufferScratch, BufferSerializer, CompositeSerializer},
    Archive, Deserialize, Serialize,
};

use crate::SCRATCH_BUF_BYTES;

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

impl PartialEq<[u8; CONTRACT_ID_BYTES]> for ContractId {
    fn eq(&self, other: &[u8; CONTRACT_ID_BYTES]) -> bool {
        self.0.eq(other)
    }
}

/// Debug implementation for [`ContractId`]
///
/// This implementation uses the normal display implementation.
impl core::fmt::Debug for ContractId {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        core::fmt::Display::fmt(self, f)
    }
}

/// Display implementation for [`ContractId`]
///
/// This implementation will display the hexadecimal representation of the bytes
/// of the [`ContractId`]. If the alternate flag is set, it will also display
/// the `0x` prefix.
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

impl TryFrom<String> for ContractId {
    type Error = core::fmt::Error;

    /// Tries to convert a hexadecimal string into a [`ContractId`]
    ///
    /// The string can be prefixed with `0x` or not.
    fn try_from(value: String) -> core::result::Result<Self, Self::Error> {
        let value = value.trim_start_matches("0x");
        let decoded = hex::decode(value).map_err(|_| core::fmt::Error)?;
        let bytes: [u8; CONTRACT_ID_BYTES] =
            decoded.try_into().map_err(|_| core::fmt::Error)?;

        Ok(ContractId::from_bytes(bytes))
    }
}

#[cfg(test)]
mod tests {
    use alloc::{format, string::ToString};

    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};

    use super::*;

    const CONTRACT_ID_STR: &str =
        "0000000000000000000000000000000000000000000000000000000000000000";
    const CONTRACT_ID_STR_PRETTY: &str =
        "0x0000000000000000000000000000000000000000000000000000000000000000";

    #[test]
    fn contract_id_display() {
        let contract_id = ContractId::from_bytes([0u8; CONTRACT_ID_BYTES]);
        assert_eq!(format!("{}", contract_id), CONTRACT_ID_STR);

        let contract_id = ContractId::from_bytes([0u8; CONTRACT_ID_BYTES]);
        assert_eq!(format!("{:#?}", contract_id), CONTRACT_ID_STR_PRETTY);
    }

    #[test]
    fn contract_id_debug() {
        let contract_id = ContractId::from_bytes([0u8; CONTRACT_ID_BYTES]);
        assert_eq!(format!("{}", contract_id), CONTRACT_ID_STR);
    }

    #[test]
    fn contract_id_to_from_string() {
        let mut rng = StdRng::seed_from_u64(1618);
        let contract_id = ContractId::from_bytes(rng.gen());

        let string = contract_id.to_string();

        assert_eq!(string.starts_with("0x"), false);
        assert_eq!(string.len(), CONTRACT_ID_BYTES * 2);

        let contract_id_from_string = ContractId::try_from(string).unwrap();

        assert_eq!(contract_id, contract_id_from_string);
    }

    #[test]
    fn contract_id_try_from_invalid_string() {
        let too_short = ContractId::try_from("0x".to_string()).is_err();

        let too_long =
            ContractId::try_from(format!("{}0", CONTRACT_ID_STR)).is_err();

        assert!(too_short);
        assert!(too_long);
    }
}
