# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Added blocking 'init' method to be called only during deployment [#433]

## [0.27.2] - 2025-02-20

### Changed

- Change `callstack` to return callers' id list with no self id at the beginning [#422]

## [0.27.1] - 2025-01-15

### Added

- Add `Session::set_meta` [#rusk_3341]
- Add `Session::remove_meta` [#rusk_3341]

## [0.27.0] - 2024-12-18

### Added

- Access to callstack via uplink API [#405]

### Changed

- Store merkle tree positions in a binary format [#410]

## [0.26.0] - 2024-10-21

### Changed

- Improved performance of commit operations [#396]
- Improved page finding performance [#393]

## [0.25.0] - 2024-09-30

### Changed

- Improved commit writing time [#388]
- Changed state representation to improve commit performance [#342]

## [0.24.0] - 2024-09-04

### Changed

- Fix behavior of calling a non-existent contract in `c` import

## [0.23.0] - 2024-08-01

### Changed

- Change `callee` import to return an integer
- Change `constructor_arg` to `init_arg`
- Change all references to `constructor` either to `init` or `initializer`

## [0.22.0] - 2024-07-03

### Added

- Added `deploy_raw` method for serialized constructor arguments [#371]

### Changed

- Make each event byte cost the same as a storage byte [#359]

### Fixed

- Fix incomplete removal of economic protocol functionality

## [0.21.0] - 2024-06-26

### Removed

- Remove economic protocol-related functionality

## [0.20.0] - 2024-06-05

### Added

- Apply charging mechanism for host queries [#359]
- Add `HostQuery::execute` and `HostQuery::deserialize_and_price` [#359]
- Add support for metadata elements: free limit and free price hint [#357] 

### Changed

- Drop `Fn(&mut [u8], u32) -> u32` bound for `HostQuery` [#359]
- Make storage instructions cost 4 gas per byte [#359]
- Upgrade `dusk-wasmtime` to version `21.0.0-alpha` [#359]

## [0.19.0] - 2024-05-08

### Added

- Add contract's charge applying mechanism [#353]
- Add contract's allowance passing and applying mechanism [#350]

## [0.18.1] - 2024-04-24

### Changed

- Upgrade `dusk-wasmtime` to version `20`

## [0.18.0] - 2024-03-27

### Added

- Add `Error::ArgumentBufferOverflow` variant [#343]

### Fixed

- Fix possible under/overflows reported by audit [#343]


### Changed

- Change `Session::feeder_call` and `Session::feeder_call_raw` to allow for the
  caller to specify the gas limit [#344]

### Fixed

- Fix `Session::migrate` to replace the contract ID in the new contract's
  metadata to the old contract's ID [#347]

## [0.17.0] - 2024-02-28

### Added

- Add `ContractDataBuilder::owner` to allow for setting the owner of a contract
  on deploy time [#336]

### Changed

- Change `migrate` to take the owner of the contract being replaced if it is
  not set by the caller [#336]
- Make `owner` field optional in `ContractData` and `ContractDataBuilder` [#336]
- Change `ContractData` and `ContractDataBuilder` to take a `Vec<u8>` as owner
  instead of `[u8; N]` [#336]
- Use empty constructor arguments by default [#316]
- Upgrade `dusk-wasmtime` to version `18`

## [0.16.0] - 2024-02-14

### Added

- Add `Session::migrate` to allow for swapping contract code [#313]

### Changed

- Upgrade `dusk-wasmtime` to version `17`
- Change `call` and `feeder_call` functions to support `bytecheck`-based integrity check of arguments [#324]

### Fixed

- Fix overflow in gas limit calculation in inter-contract call

## [0.15.0] - 2024-01-24

### Changed

- Change `owner` import to accept the contract ID as argument and return
  non-zero upon success

## [0.14.1] - 2024-01-11

### Fixed

- Fix module recompilation on invalid object code

## [0.14.0] - 2023-12-13

### Added

- Use `ContractError::to_parts` to write error messages to the argument buffer [#301]

### Changed

- Change documentation to change terminology from `points` to `gas`
- Rename `CallReceipt::points_limit` and `CallReceipt::points_spent` to
  `CallReceipt::gas_limit` and `CallReceipt::gas_spent` respectively
- Rename `Error::OutOfPoints` to `Error::OutOfGas`
- Rename `Error::ContractPanic` to `Error::Panic` to be more clear that the entire
  execution panicked [#301]
- Upgrade `dusk-wasmtime` to version `15`
- De-instantiate modules after call [#296]
- Change `Session::memory_len` to return `Result<Option<usize>>`, and not
  require a contract to be instantiated [#296]

### Removed

- Remove `once_cell` dependency

### Fixed

- Fix improper use of mach_ports
- Fix inconsistent state root after erroring call [#296]

## [0.13.0] - 2023-11-22

## Added

- Add `Session::memory_pages` allowing for inclusion proofs of pages [#273]

## Changed

- Change state tree to distinguish between 32 and 64 bit smart contracts [#273]

## [0.12.0] - 2023-11-01

## Added

- Support `memory64` smart contracts [#281]
- Add some `Error` variants:
    * `InvalidFunction`
    * `InvalidMemory`
- Add `once_cell` dependency

## Changed

- Upgrade `dusk-merkle` to version `0.5`
- Change contract tree to be arity 4 and height 17 [#159]
- Maximum contract size is now 4TiB [#159]
- Change `Error::RuntimeError` variant to contain `dusk_wasmtime::Error`,
  and changed `From` implementation
- Switch runtime from `wasmer` to `wasmtime`

## Removed

- Remove `parking_lot` dependency
- Remove `colored` dependency
- Remove 4 page - 256KiB - minimum memory requirement for contracts
- Remove `Clone` derivation for `Error`
- Remove some `Error` variants, along with `From` implementations:
    * `CompileError`
    * `DeserializeError`
    * `ExportError`
    * `InstantiationError`
    * `InvalidFunctionSignature`
    * `MemorySetupError`
    * `ParsingError`
    * `SerializeError`
    * `Trap`

## Fixed

- Fix loading of compiled contracts from state transported from different
  platforms [#287]

## [0.11.0] - 2023-10-11

### Added

- Add `spent` field to `CallTreeElem` [#206]
- Add `call_tree` to `CallReceipt` [#206]
- Expose `CallTree` and `CallTreeElem` in the public API [#206]
- Add `CallTreeIter` to improve iteration over call tree [#206]
- Add `panic` import implementation [#271]
- Add `Error::ContractPanic` variant [#271]

### Changed

- Adapt to use `LocateFile` - `crumbles`'s lazy page loading mechanism
- Adapt to `crumbles` needing `n_pages` and `page_size`
- Change return of `owner` and `self_id` to `()`
- Rename `StackElement` to `CallTreeElem` [#206]
- Allow for multiple initializations on a new memory [#271]
- Downcast `Error::RuntimeError` on each call boundary [#271]

### Removed

- Remove `CallStack` in favor of `CallTree` [#206]

## [0.10.0] - 2023-09-13

### Added

- Add `Session::memory_len` to get the length of a memory in session [#268]

### Changed

- Change minimum number of pages to be 4
- Change reporting of memory to the host to be the total range of the memory
  mapping available

### Removed

- Fake guard pages are now removed

### Fixed

- Revert memory size on errors [#268]
- Fix reporting of memory size to `wasmer` [#268]

## [0.9.3] - 2023-09-07

### Fixed

- Fix out of bound in argument buffer handling

## [0.9.2] - 2023-09-07

### Changed

- Change to use `crumbles::Mmap::set_len` on growing memory

## [0.9.1] - 2023-09-07

### Changed

- Remove re-execution in favor of micro-snapshots [#254]

### Fixed

- Fix non-existing memory directory when not modifying a contract

## [0.9.0] - 2023-08-30

### Changed

- Change commit write behavior to write dirty pages instead of diffs [#253]
- Change memory backend to use `crumbles` instead of `libc` directly  [#253]

### Removed

- Remove `Session::squash_commit`  since it's irrelevant with the new commit behavior [#253]
- Remove `libc` dependency [#253]
- Remove `flate2` dependency [#253]
- Remove `qbsdiff` dependency [#253]

## [0.8.0] - 2023-08-09

### Added

- Add `Error::MemoryAccessOutOfBounds` [#249]
- Add `memmap2` dependency

### Changed

- Change imports
- Change diffing algorithm to not delegate growth to `bsdiff`
- Change memory growth algorithm to not require copying to temp file

### Fixed

- Fix behavior of imports on out of bounds pointers [#249]
- Fix SIGBUS caused by improper memory growth

## [0.7.0] - 2023-07-19

### Added

- Add support for the `feed` import [#243]
- Add `Error::Infallible` variant
- Add `Error::MissingHostData` variant
- Add `Error::MissingHostQuery` variant
- Add `Error::Utf8` variant
- Add `CallReceipt` struct

### Changed

- Change signature of `SessionDataBuilder::insert` to return an error on serialization
- Handle possible errors in imports
- Handle error on deserializing contract metadata
- Change signature of `Session::deploy` to take `points_limit`
- Change signature of `Session::call` to take `points_limit`
- Change signature of `Session::call_raw` to take `points_limit`
- Change signature of `Session::call` to return `CallReceipt`
- Change signature of `Session::call_raw` to return `CallReceipt`

### Removed

- Remove `Session::set_point_limit`
- Remove `Session::take_events`
- Remove `Session::spent`

## [0.6.2] - 2023-07-07

### Added

- Add `ContractDoesNotExist` variant to the `Error` enum

### Change

- Error instead of panicking on making a call to non-existing contract

## [0.6.1] - 2023-06-28

### Added

- Re-export the entirety of `piecrust-uplink` [#234]

### Change

- Allow for `piecrust-uplink` version variability [#234]

## [0.6.0] - 2023-06-28

### Added

- Add `debug` feature, gating debugging capabilities [#222]

### Changed

- Change event handling to emit `piecrust-uplink::Event`
- Change `emit` export to include topic
- Remove `Into<PathBuf>` bound in `VM::new`
- Rename `host_debug` export to `hdebug` [#222]

### Fixed

- Fix memleak due to last contract instance not being reclaimed
  in session.

### Removed

- Remove `Event` struct
- Remove `__heap_base` requirement from contracts

## [0.5.0] - 2023-06-07

### Added

- Add `Session::call_raw` allowing for deferred (de)serialization [#218]
- Add `MAP_NORESERVE` flag to `mmap` syscall [#213]

### Changed

- Include `points_limit` in `c` import [#216]

## [0.4.0] - 2023-05-17

### Added

- Add `RawCall` re-export [#136]
- Add `Session::call` [#136]
- Add crate-specific README. [#174]

### Changed

- Change `owner` parameter type in `ModuleData::builder` to be `[u8; N]` [#201]

### Fixed

- Fix SIGSEGV caused by moving sessions with instantiate modules [#202]

### Removed

- Remove `RawQuery/Transact` re-rexports [#136]
- Remove `Session::query/transact` [#136]
- Remove `query/transact` imports [#136]

## [0.3.0] - 2023-04-26

### Changed

- Change `module` named functions and items to `contract` [#207]
- Store module Merkle tree [#166]
- Rename `DeployData` to `ModuleData`

### Removed

- Remove `VM::genesis_session` in favor of config parameters in `VM::session`

## [0.2.0] - 2023-04-06

### Added

- Added uplink::owner and uplink::self_id. [#158]
- Implemented Display for ModuleId. [#178]
- Added persistence for module metadata. [#167]
- Added `DeployData` and `DeployDataBuilder`. [#158]
- Added contract constructor mechanism. [#93]
- Added caching of module compilation outputs. [#162]
- Derive `Debug` for `Session` and `VM`

### Changed

- Made session data settable only at deploy time. [#181]
- Changed deploy API to accept `Into<DeployData>`. [#158]
- Made modules compile at deploy time rather than on first query/transaction time. [#162]

### Removed

- Removed errant print statements.
- Removed SELF_ID export from contracts. [#167]

## [0.1.0] - 2023-03-15

- First `piecrust` release

<!-- PULLS -->

[#234]: https://github.com/dusk-network/piecrust/pull/234

<!-- ISSUES -->

[#rusk_3341]: https://github.com/dusk-network/rusk/issues/3341
[#433]: https://github.com/dusk-network/piecrust/issues/433
[#422]: https://github.com/dusk-network/piecrust/issues/422
[#410]: https://github.com/dusk-network/piecrust/issues/410
[#405]: https://github.com/dusk-network/piecrust/issues/405
[#396]: https://github.com/dusk-network/piecrust/issues/396
[#393]: https://github.com/dusk-network/piecrust/issues/393
[#388]: https://github.com/dusk-network/piecrust/issues/388
[#371]: https://github.com/dusk-network/piecrust/issues/371
[#359]: https://github.com/dusk-network/piecrust/issues/359
[#357]: https://github.com/dusk-network/piecrust/issues/357
[#353]: https://github.com/dusk-network/piecrust/issues/353
[#350]: https://github.com/dusk-network/piecrust/issues/350
[#347]: https://github.com/dusk-network/piecrust/issues/347
[#344]: https://github.com/dusk-network/piecrust/issues/344
[#343]: https://github.com/dusk-network/piecrust/issues/343
[#342]: https://github.com/dusk-network/piecrust/issues/342
[#336]: https://github.com/dusk-network/piecrust/issues/336
[#325]: https://github.com/dusk-network/piecrust/issues/325
[#324]: https://github.com/dusk-network/piecrust/issues/324
[#316]: https://github.com/dusk-network/piecrust/issues/316
[#301]: https://github.com/dusk-network/piecrust/issues/313
[#301]: https://github.com/dusk-network/piecrust/issues/301
[#296]: https://github.com/dusk-network/piecrust/issues/296
[#287]: https://github.com/dusk-network/piecrust/issues/287
[#281]: https://github.com/dusk-network/piecrust/issues/281
[#273]: https://github.com/dusk-network/piecrust/issues/273
[#271]: https://github.com/dusk-network/piecrust/issues/271
[#268]: https://github.com/dusk-network/piecrust/issues/268
[#254]: https://github.com/dusk-network/piecrust/issues/254
[#253]: https://github.com/dusk-network/piecrust/issues/253
[#249]: https://github.com/dusk-network/piecrust/issues/249
[#243]: https://github.com/dusk-network/piecrust/issues/243
[#222]: https://github.com/dusk-network/piecrust/issues/222
[#218]: https://github.com/dusk-network/piecrust/issues/218
[#216]: https://github.com/dusk-network/piecrust/issues/216
[#213]: https://github.com/dusk-network/piecrust/issues/213
[#207]: https://github.com/dusk-network/piecrust/issues/207
[#206]: https://github.com/dusk-network/piecrust/issues/206
[#202]: https://github.com/dusk-network/piecrust/issues/202
[#201]: https://github.com/dusk-network/piecrust/issues/201
[#181]: https://github.com/dusk-network/piecrust/issues/181
[#178]: https://github.com/dusk-network/piecrust/issues/178
[#174]: https://github.com/dusk-network/piecrust/issues/174
[#167]: https://github.com/dusk-network/piecrust/issues/167
[#166]: https://github.com/dusk-network/piecrust/issues/166
[#162]: https://github.com/dusk-network/piecrust/issues/162
[#159]: https://github.com/dusk-network/piecrust/issues/159
[#158]: https://github.com/dusk-network/piecrust/issues/158
[#136]: https://github.com/dusk-network/piecrust/issues/136
[#93]: https://github.com/dusk-network/piecrust/issues/93

<!-- VERSIONS -->

[Unreleased]: https://github.com/dusk-network/piecrust/compare/piecrust-0.27.2...HEAD
[0.27.2]: https://github.com/dusk-network/piecrust/compare/piecrust-0.27.1...piecrust-0.27.2
[0.27.1]: https://github.com/dusk-network/piecrust/compare/piecrust-0.27.0...piecrust-0.27.1
[0.27.0]: https://github.com/dusk-network/piecrust/compare/piecrust-0.26.0...piecrust-0.27.0
[0.26.0]: https://github.com/dusk-network/piecrust/compare/piecrust-0.25.0...piecrust-0.26.0
[0.25.0]: https://github.com/dusk-network/piecrust/compare/piecrust-0.24.0...piecrust-0.25.0
[0.24.0]: https://github.com/dusk-network/piecrust/compare/piecrust-0.23.0...piecrust-0.24.0
[0.23.0]: https://github.com/dusk-network/piecrust/compare/piecrust-0.22.0...piecrust-0.23.0
[0.22.0]: https://github.com/dusk-network/piecrust/compare/piecrust-0.21.0...piecrust-0.22.0
[0.21.0]: https://github.com/dusk-network/piecrust/compare/piecrust-0.20.0...piecrust-0.21.0
[0.20.0]: https://github.com/dusk-network/piecrust/compare/piecrust-0.19.0...piecrust-0.20.0
[0.19.0]: https://github.com/dusk-network/piecrust/compare/piecrust-0.18.1...piecrust-0.19.0
[0.18.1]: https://github.com/dusk-network/piecrust/compare/piecrust-0.18.0...piecrust-0.18.1
[0.18.0]: https://github.com/dusk-network/piecrust/compare/piecrust-0.17.0...piecrust-0.18.0
[0.17.0]: https://github.com/dusk-network/piecrust/compare/piecrust-0.16.0...piecrust-0.17.0
[0.16.0]: https://github.com/dusk-network/piecrust/compare/piecrust-0.15.0...piecrust-0.16.0
[0.15.0]: https://github.com/dusk-network/piecrust/compare/piecrust-0.14.1...piecrust-0.15.0
[0.14.1]: https://github.com/dusk-network/piecrust/compare/piecrust-0.14.0...piecrust-0.14.1
[0.14.0]: https://github.com/dusk-network/piecrust/compare/piecrust-0.13.0...piecrust-0.14.0
[0.13.0]: https://github.com/dusk-network/piecrust/compare/piecrust-0.12.0...piecrust-0.13.0
[0.12.0]: https://github.com/dusk-network/piecrust/compare/piecrust-0.11.0...piecrust-0.12.0
[0.11.0]: https://github.com/dusk-network/piecrust/compare/piecrust-0.10.0...piecrust-0.11.0
[0.10.0]: https://github.com/dusk-network/piecrust/compare/piecrust-0.9.3...piecrust-0.10.0
[0.9.3]: https://github.com/dusk-network/piecrust/compare/piecrust-0.9.2...piecrust-0.9.3
[0.9.2]: https://github.com/dusk-network/piecrust/compare/piecrust-0.9.1...piecrust-0.9.2
[0.9.1]: https://github.com/dusk-network/piecrust/compare/piecrust-0.9.0...piecrust-0.9.1
[0.9.0]: https://github.com/dusk-network/piecrust/compare/piecrust-0.8.0...piecrust-0.9.0
[0.8.0]: https://github.com/dusk-network/piecrust/compare/v0.7.0...piecrust-0.8.0
[0.7.0]: https://github.com/dusk-network/piecrust/compare/piecrust-0.6.2...v0.7.0
[0.6.1]: https://github.com/dusk-network/piecrust/compare/piecrust-0.6.1...piecrust-0.6.2
[0.6.1]: https://github.com/dusk-network/piecrust/compare/v0.6.0...piecrust-0.6.1
[0.6.0]: https://github.com/dusk-network/piecrust/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/dusk-network/piecrust/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/dusk-network/piecrust/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/dusk-network/piecrust/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/dusk-network/piecrust/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/dusk-network/piecrust/releases/tag/v0.1.0
