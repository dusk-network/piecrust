// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::path::Path;
use std::sync::Arc;

use wasmer::wasmparser::Operator;
use wasmer::{BaseTunables, CompilerConfig, Store, Target, Universal};
use wasmer_compiler_singlepass::Singlepass;
use wasmer_middlewares::Metering;

fn cost_function(_: &Operator) -> u64 {
    0
}

/// Creates a new store using the singlepass compiler configured to meter using
/// the default cost function.
pub fn new_store<P: AsRef<Path>>(path: P) -> Store {
    let mut compiler_config = Singlepass::default();
    let metering = Arc::new(Metering::new(0, cost_function));

    compiler_config.push_middleware(metering);

    Store::new_with_tunables_and_path(
        &Universal::new(compiler_config).engine(),
        BaseTunables::for_target(&Target::default()),
        path.as_ref().into(),
    )
}
