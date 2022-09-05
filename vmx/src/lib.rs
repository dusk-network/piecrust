// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

#[macro_use]
mod bytecode_macro;
mod instance;
mod module;
mod session;
mod types;
mod vm;

pub use vm::VM;
