// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use alloc::string::String;

use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{ContractId, Event, CONTRACT_ID_BYTES};

impl Serialize for ContractId {
    fn serialize<S: Serializer>(
        &self,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        hex::serde::serialize(&self.to_bytes(), serializer)
    }
}

impl<'de> Deserialize<'de> for ContractId {
    fn deserialize<D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Self, D::Error> {
        let bytes: [u8; CONTRACT_ID_BYTES] =
            hex::serde::deserialize(deserializer)?;
        Ok(bytes.into())
    }
}

impl Serialize for Event {
    fn serialize<S: Serializer>(
        &self,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let mut struct_ser = serializer.serialize_struct("Event", 3)?;
        struct_ser.serialize_field("source", &self.source)?;
        struct_ser.serialize_field("topic", &self.topic)?;
        struct_ser.serialize_field("data", &hex::encode(&self.data))?;
        struct_ser.end()
    }
}

impl<'de> Deserialize<'de> for Event {
    fn deserialize<D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct IntermediateEvent {
            source: ContractId,
            topic: String,
            data: String,
        }

        let intermediate: IntermediateEvent =
            Deserialize::deserialize(deserializer)?;
        let data = hex::decode(&intermediate.data)
            .map_err(serde::de::Error::custom)?;
        Ok(Event {
            source: intermediate.source,
            topic: intermediate.topic,
            data,
        })
    }
}
