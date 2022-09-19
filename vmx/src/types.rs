// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use rkyv::ser::serializers::{
    BufferScratch, BufferSerializer, CompositeSerializer,
};
use uplink::SCRATCH_BUF_BYTES;

pub type StandardBufSerializer<'a> = CompositeSerializer<
    BufferSerializer<&'a mut [u8]>,
    BufferScratch<&'a mut [u8; SCRATCH_BUF_BYTES]>,
>;

pub type Error = Box<dyn std::error::Error>;
