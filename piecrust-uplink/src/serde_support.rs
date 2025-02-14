// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use alloc::format;
use alloc::string::String;

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use serde::de::{Error as SerdeError, MapAccess, Visitor};
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
        let bytes: [u8; CONTRACT_ID_BYTES] = hex::serde::deserialize(deserializer)?;
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
        struct_ser
            .serialize_field("data", &BASE64_STANDARD.encode(&self.data))?;
        struct_ser.end()
    }
}

impl<'de> Deserialize<'de> for Event {
    fn deserialize<D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Self, D::Error> {
        struct StructVisitor;

        impl<'de> Visitor<'de> for StructVisitor {
            type Value = Event;

            fn expecting(
                &self,
                formatter: &mut alloc::fmt::Formatter,
            ) -> alloc::fmt::Result {
                formatter
                    .write_str("a struct with fields: source, topic, and data")
            }

            fn visit_map<A: MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let (mut source, mut topic, mut data) = (None, None, None);
                while let Some(key) = map.next_key()? {
                    match key {
                        "source" => {
                            if source.is_some() {
                                return Err(SerdeError::duplicate_field(
                                    "source",
                                ));
                            }
                            source = Some(map.next_value()?);
                        }
                        "topic" => {
                            if topic.is_some() {
                                return Err(SerdeError::duplicate_field(
                                    "topic",
                                ));
                            }
                            topic = Some(map.next_value()?);
                        }
                        "data" => {
                            if data.is_some() {
                                return Err(SerdeError::duplicate_field(
                                    "data",
                                ));
                            }
                            data = Some(map.next_value()?);
                        }
                        field => {
                            return Err(SerdeError::unknown_field(
                                field,
                                &["source", "topic", "data"],
                            ))
                        }
                    };
                }
                let data: String =
                    data.ok_or_else(|| SerdeError::missing_field("data"))?;
                let data = BASE64_STANDARD.decode(data).map_err(|e| {
                    SerdeError::custom(format!(
                        "failed to base64 decode Event data: {e}"
                    ))
                })?;
                Ok(Event {
                    source: source
                        .ok_or_else(|| SerdeError::missing_field("source"))?,
                    topic: topic
                        .ok_or_else(|| SerdeError::missing_field("topic"))?,
                    data,
                })
            }
        }

        deserializer.deserialize_struct(
            "Event",
            &["source", "topic", "data"],
            StructVisitor,
        )
    }
}
