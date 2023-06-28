# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

<!-- ISSUES -->
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
[Unreleased]: https://github.com/dusk-network/piecrust/compare/v0.6.0...HEAD
[0.6.0]: https://github.com/dusk-network/piecrust/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/dusk-network/piecrust/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/dusk-network/piecrust/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/dusk-network/piecrust/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/dusk-network/piecrust/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/dusk-network/piecrust/releases/tag/v0.1.0
