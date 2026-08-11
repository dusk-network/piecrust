// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Host-backed contract ABI.
//!
//! Contract exports retain the standard `(u32) -> u32` signature, while call
//! data and host interactions use dynamically allocated memory instead of the
//! legacy fixed argument buffer.

use rkyv::ser::serializers::AllocSerializer;

use crate::SCRATCH_BUF_BYTES;

#[cfg(not(feature = "abi"))]
#[path = "abi/allocator.rs"]
mod allocator;

mod externs;

mod host;
pub use host::*;

mod helpers;
pub use helpers::*;

mod state;
pub use state::*;

#[cfg(all(
    not(feature = "abi"),
    any(target_arch = "wasm32", target_arch = "wasm64")
))]
mod handlers;

#[cfg(feature = "debug")]
mod debug;
#[cfg(feature = "debug")]
pub use debug::*;

/// Dynamic serializer used by the host-backed ABI.
pub type HostBufSerializer = AllocSerializer<SCRATCH_BUF_BYTES>;

mod marker {
    /// Marker used by the host to select the host-backed ABI.
    #[used]
    #[unsafe(no_mangle)]
    static B: u8 = 0;
}

#[cfg(test)]
mod tests;
