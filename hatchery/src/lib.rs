// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.
#![cfg_attr(feature = "test", feature(test, bigint_helper_methods))]

// mod env;
// mod error;
mod hash;
// mod instance;
// mod memory;
mod session;
// mod snapshot;
// mod storage_helpers;
// mod world;
//
// pub use error::Error;
// pub use world::{Event, NativeQuery, Receipt, World};

#[macro_export]
macro_rules! module_bytecode {
    ($name:literal) => {
        include_bytes!(concat!(
            "../../modules/target/wasm32-unknown-unknown/release/",
            $name,
            ".wasm"
        ))
    };
}
