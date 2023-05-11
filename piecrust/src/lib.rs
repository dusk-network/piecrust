// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Piecrust VM for WASM smart-contract execution.
//!
//! A [`VM`] is instantiated by calling [`VM::new`] using a directory for
//! storage of commits.
//!
//! Once instantiation has been successful, [`Session`]s can be started using
//! [`VM::session`]. A session represents the execution of a sequence of
//! [`query`], [`transact`], and [`deploy`] calls, and stores mutations to the
//! underlying state as a result. This sequence of mutations may be committed -
//! meaning written to the VM's directory - using [`commit`]. After a commit,
//! the resulting state may be used by starting a new session with it as a base.
//!
//! Contract execution is metered in terms of `points`. To set a limit for the
//! number of points for the next query or transact call use
//! [`set_point_limit`]. If the limit is exceeded during the call an error will
//! be returned. To learn the number of points spent after a successful call use
//! [`spent`]. To learn more about the compiler middleware used to achieve this,
//! please refer to the relevant [wasmer docs].
//!
//! # State Representation and Session/Commit Mechanism
//!
//! Smart Contracts are represented on disk by two separate files: their WASM
//! bytecode and their linear memory at a given commit. The collection of all
//! the memories of smart contracts at a given commit is referred to as the
//! *state* of said commit.
//!
//! During a session, each contract called in the sequence of
//! queries/transactions is loaded by:
//!
//! - Reading the contract's bytecode file
//! - Memory mapping the linear memory file copy-on-write (CoW)
//!
//! Using copy-on-write mappings of linear memories ensures that each commit is
//! never mutated in place by a session, with the important exception of
//! [`deletions`] and [`squashes`].
//!
//! # Session Concurrency
//!
//! Multiple sessions may be started concurrently on the same `VM`, and then
//! passed on to different threads. These sessions are then non-overlapping
//! sequences of mutations of a state and may all be committed/dropped
//! simultaneously.
//!
//! ```
//! use piecrust::{Session, VM};
//!
//! fn assert_send<T: Send>() {}
//!
//! // Both VM and Session are `Send`
//! assert_send::<VM>();
//! assert_send::<Session>();
//! ```
//!
//! This is achieved by synchronizing commit deletions, squashes, and session
//! spawns/commits using a synchronization loop started on VM instantiation.
//!
//! # Call Atomicity
//!
//! Contract calls are executed atomically, that is, they are either executed
//! completely or they are not executed at all.
//!
//! In other words, if the call succeeds, all the state mutations they produce
//! are kept, while if they produce an error (e.g. they panic), all such
//! mutations are reverted.
//!
//! If the call was made within another call (i.e., the caller was a contract),
//! we ensure all mutations are reverted by undoing the whole call stack of the
//! current transact/query execution, and re-executing it with the exception of
//! the error-producing call, which returns an error without being actually
//! executed.
//!
//! # Usage
//! ```
//! use piecrust::{module_bytecode, ModuleData, SessionData, VM};
//! let mut vm = VM::ephemeral().unwrap();
//!
//! const OWNER: [u8; 32] = [0u8; 32];
//! let mut session = vm.session(SessionData::builder()).unwrap();
//! let counter_id = session.deploy(module_bytecode!("counter"), ModuleData::builder(OWNER)).unwrap();
//!
//! assert_eq!(session.call::<(), i64>(counter_id, "read_value", &()).unwrap(), 0xfc);
//! session.call::<(), ()>(counter_id, "increment", &()).unwrap();
//! assert_eq!(session.call::<(), i64>(counter_id, "read_value", &()).unwrap(), 0xfd);
//!
//! let commit_root = session.commit().unwrap();
//! assert_eq!(commit_root, vm.commits()[0]);
//! ```
//!
//! [`VM`]: VM
//! [`VM::new`]: VM::new
//! [`Session`]: Session
//! [`VM::session`]: VM::session
//! [`query`]: Session::query
//! [`transact`]: Session::transact
//! [`deploy`]: Session::deploy
//! [`commit`]: Session::commit
//! [`set_point_limit`]: Session::set_point_limit
//! [`spent`]: Session::spent
//! [wasmer docs]: wasmer_middlewares::metering
//! [`deletions`]: VM::delete_commit
//! [`squashes`]: VM::squash_commit

#[macro_use]
mod bytecode_macro;
mod error;
mod event;
mod imports;
mod instance;
mod module;
mod session;
mod store;
mod types;
mod vm;

pub use error::Error;
pub use module::{ModuleData, ModuleDataBuilder};
pub use session::{Session, SessionData};
pub use vm::{HostQuery, VM};

// re-exports

pub use piecrust_uplink::{ModuleId, RawCall};
