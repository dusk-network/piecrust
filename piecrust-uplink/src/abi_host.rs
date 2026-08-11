// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Host-backed contract ABI.
//!
//! Contract exports retain the standard `(u32) -> u32` signature. Contract
//! calls and host queries use host-backed frames, while bounded operations
//! reuse the legacy imports through the `B` scratch buffer.

use rkyv::ser::serializers::AllocSerializer;

use crate::SCRATCH_BUF_BYTES;
#[cfg(feature = "debug")]
pub use crate::abi::hdebug;
pub use crate::abi::{
    ArgbufWriter, caller, callstack, emit, emit_raw, feed, feed_raw, limit,
    meta_data, owner, self_id, self_owner, spent,
};

mod externs;

mod host;
pub use host::*;

mod helpers;
pub use helpers::*;

mod state;
pub use state::*;

/// Dynamic serializer used for host-backed call and query payloads.
pub type HostBufSerializer = AllocSerializer<SCRATCH_BUF_BYTES>;

#[cfg(test)]
mod tests;
