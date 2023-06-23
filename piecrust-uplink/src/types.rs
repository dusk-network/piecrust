// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use alloc::string::String;
use alloc::vec::Vec;

use bytecheck::CheckBytes;
use rkyv::{
    ser::serializers::{
        AllocSerializer, BufferScratch, BufferSerializer, CompositeSerializer,
    },
    ser::Serializer,
    Archive, Deserialize, Infallible, Serialize,
};

use crate::SCRATCH_BUF_BYTES;

/// The target of an event.
///
/// Events emitted by contracts are always of the [`Contract`] variant.
///
/// [`Contract`]: [`EventTarget::Contract`]
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
pub enum EventTarget {
    /// The event targets a contract.
    Contract(ContractId),
    /// The event targets the host machine.
    Host(String),
    /// The event is a debug event.
    Debugger(String),
}

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
    pub target: EventTarget,
    pub topic: String,
    pub data: Vec<u8>,
    pub cancelable: bool,
    pub capturable: bool,
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

/// A `RawCall` is a contract call that doesn't care about types and only
/// operates on raw data.
#[derive(Archive, Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
#[archive_attr(derive(CheckBytes))]
pub struct RawCall {
    arg_len: u32,
    data: alloc::vec::Vec<u8>,
}

impl RawCall {
    /// Creates a new [`RawCall`] by serializing an argument.
    ///
    /// The name of the [`RawCall`] is stored in its `data` field after the
    /// arguments.
    pub fn new<A>(name: &str, arg: A) -> Self
    where
        A: Serialize<AllocSerializer<64>>,
    {
        let mut ser = AllocSerializer::default();

        ser.serialize_value(&arg)
            .expect("We assume infallible serialization and allocation");

        let data = ser.into_serializer().into_inner().to_vec();
        Self::from_parts(name, data)
    }

    /// Create a new [`RawCall`] from its parts without serializing data.
    ///
    /// This assumes the `data` given has already been correctly serialized for
    /// the contract to call.
    pub fn from_parts(name: &str, data: alloc::vec::Vec<u8>) -> Self {
        let mut data = data;

        let arg_len = data.len() as u32;
        data.extend_from_slice(name.as_bytes());

        Self { arg_len, data }
    }

    /// Return a reference to the name of [`RawCall`]
    pub fn name(&self) -> &str {
        core::str::from_utf8(self.name_bytes())
            .expect("always created from a valid &str")
    }

    /// Return a reference to the raw name of [`RawCall`]
    pub fn name_bytes(&self) -> &[u8] {
        &self.data[self.arg_len as usize..]
    }

    /// Return a reference to the raw argument of [`RawCall`]
    pub fn arg_bytes(&self) -> &[u8] {
        &self.data[..self.arg_len as usize]
    }
}

#[derive(Archive, Serialize, Deserialize)]
#[archive_attr(derive(CheckBytes))]
pub struct RawResult {
    data: alloc::vec::Vec<u8>,
}

/// A RawResult is a result that doesn't care about its type and only
/// operates on raw data.
impl RawResult {
    /// Creates a new [`RawResult`] from raw data as bytes.
    pub fn new(bytes: &[u8]) -> Self {
        RawResult {
            data: alloc::vec::Vec::from(bytes),
        }
    }

    /// Casts the `data` from [`RawResult`] to the desired type by serializing
    /// and returning it
    pub fn cast<D>(&self) -> D
    where
        D: Archive,
        D::Archived: Deserialize<D, Infallible>,
    {
        // add bytecheck here.
        let archived = unsafe { rkyv::archived_root::<D>(&self.data[..]) };
        archived.deserialize(&mut Infallible).expect("Infallible")
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn raw_call() {
        let q = RawCall::new("hello", 42u128);

        assert_eq!(q.name(), "hello");
        assert_eq!(
            q.arg_bytes(),
            [
                0x2a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00
            ]
        );
    }
}
