# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Change commit write behavior to  write dirty pages instead of diffs [#253]
- Change memory backend  to use `crumbles` instead of `libc` directly  [#253]

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

- Fix  behavior of imports on  out of bounds pointers [#249]
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
[#253]: https://github.com/dusk-network/piecrust/issues/253
[#249]: https://github.com/dusk-network/piecrust/issues/249
[#243]: https://github.com/dusk-network/piecrust/issues/243
[#222]: https://github.com/dusk-network/piecrust/issues/222
[#218]: https://github.com/dusk-network/piecrust/issues/218
[#216]: https://github.com/dusk-network/piecrust/issues/216
[#213]: https://github.com/dusk-network/piecrust/issues/213
[#207]: https://github.com/dusk-network/piecrust/issues/207
[#202]: https://github.com/dusk-network/piecrust/issues/202
[#201]: https://github.com/dusk-network/piecrust/issues/201
[#181]: https://github.com/dusk-network/piecrust/issues/181
[#178]: https://github.com/dusk-network/piecrust/issues/178
[#174]: https://github.com/dusk-network/piecrust/issues/174
[#167]: https://github.com/dusk-network/piecrust/issues/167
[#166]: https://github.com/dusk-network/piecrust/issues/166
[#162]: https://github.com/dusk-network/piecrust/issues/162
[#158]: https://github.com/dusk-network/piecrust/issues/158
[#136]: https://github.com/dusk-network/piecrust/issues/136
[#93]: https://github.com/dusk-network/piecrust/issues/93

<!-- VERSIONS -->
[Unreleased]: https://github.com/dusk-network/piecrust/compare/piecrust-0.8.0...HEAD
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
