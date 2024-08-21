# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Add `ContractError::DoesNotExist` variant

## [0.16.0] - 2024-08-01

### Added

- Add `impl PartialEq<[u8; 32]> for ContractId` [#375]

### Changed

- Change `callee` function to return `Option<ContractId>`
- Change `callee` ABI to return an integer

## [0.15.0] - 2024-07-03

### Fixed

- Fix incomplete removal of economic protocol functionality

## [0.14.0] - 2024-06-26

### Removed

- Remove all economic protocol-related functionality
- Removed the 'charge' portion of the economic protocol [#365]

## [0.13.0] - 2024-06-05

### Added

- Add support for metadata elements: free limit and free price hint [#357]

## [0.12.0] - 2024-05-08

### Added

- Add contract charge setting method [#353] 
- Add contract allowance setting method and a corresponding passing mechanism [#350] 

## [0.11.0] - 2024-02-14

### Added

- Add `wrap_call_unchecked` function for calls with no argument checking [#324]

### Changed

- Change `wrap_call` function to support `bytecheck`-based integrity check of arguments [#324]

## [0.10.0] - 2024-01-24

### Added

- Add `self_owner` function returning the owner of the calling contract

### Changed

- Change `owner` function to take a `ContractId` as argument and return an `Option<[u8; N]>`
- Change `owner` extern to take a `*const u8` argument signifying the contract ID

## [0.9.0] - 2023-12-13

### Added

- Add `ContractError::to_parts` and `ContractError::from_parts` functions [#301]
- Add `fn_name` and `fn_arg` as an argument to `call_raw` and `call_raw_with_limit` [#301]

### Changed

- Change variable names and documentation to match the `gas` terminology as
  opposed to `points`
- Rename `ContractError::Other` to `ContractError::Unknown` [#301]
- Change `Display` for `ContractError` to display messages [#301]
- Change `ContractError` variants to be CamelCase [#301]

### Removed

- Remove `ContractError::from_code` function [#301]
- Remove `raw_call` as an argument of `call_raw` and `call_raw_with_limit` [#301]
- Remove `RawCall` and `RawResult` [#301]

## [0.8.0] - 2023-10-11

### Added

- Add call to `panic` in panic handler [#271]
- Add `panic` extern [#271]

### Changed

- Change return of `owner` and `self_id` to `()`

### Removed

- Remove call to `hdebug` on panic [#271]

## [0.7.1] - 2023-09-13

### Added

- Expose `arg_buf::with_arg_buf` to allow for custom argument buffer handling [#268]

## [0.7.0] - 2023-07-19

### Added

- Add contract validation during deployment [#157]
- Add more comprehensive documentation of the whole crate [#189]&[#190]
- Add `feed` extern [#243]

### Changed

- Rename `Event::target` to `Event::source` [#243]

### Removed

- Remove `EventTarget` struct [#243]

## [0.6.1] - 2023-07-03

### Fixed

- Prevent macro expansion in panic handler without the debug feature

## [0.6.0] - 2023-06-28

### Added

- Add `Event` struct according to spec
- Impl `fmt::Display` for `ContractError`
- Emit debug line on panic handler [#222]
- Add `debug` feature [#222]

### Changed

- Change `emit` extern to include topic
- Rename `MODULE_ID_BYTES` to `CONTRACT_ID_BYTES`
- Expose `extern`s only on feature `abi` [#222]
- Rename `std` feature to `abi` [#222]
- Rename `host_debug` to `hdebug` and use arg buffer [#222]

### Removed

- Remove unused `height` extern [#222]
- Remove unused `snap` extern [#222]

## [0.5.0] - 2023-06-07

### Added

- Add `call_with_limit` and `call_raw_with_limit` [#216]

### Changed

- Include `points_limit` in the `c` external [#216]

## [0.4.0] - 2023-05-17

### Added

- Add `call` and associated functions and structs [#136]
- Add `dlmalloc` feature [#199]
- Add crate-specific README [#174]

### Changed

- Change `module` named functions and items to `contract` [#207]
- Change signature of `owner` to return `[u8; N]` instead of `[u8; 32]` [#201] 

### Removed

- Remove `State` struct [#209]
- Remove `query` and `transact` and associated functions and structs [#136]
- Remove `wee_alloc` feature [#199]

## [0.3.0] - 2023-04-26

### Added

- Add documentation for `piecrust-uplink::types` [#139]

### Changed

- Rename `ModuleError` enum variants to be upper case

### Removed

- Remove deprecated `alloc_error_handler` [#192]

## [0.1.0] - 2023-03-15

- First `piecrust-uplink` release

<!-- ISSUES -->
[#375]: https://github.com/dusk-network/piecrust/issues/375
[#365]: https://github.com/dusk-network/piecrust/issues/365
[#357]: https://github.com/dusk-network/piecrust/issues/357
[#353]: https://github.com/dusk-network/piecrust/issues/353
[#350]: https://github.com/dusk-network/piecrust/issues/350
[#324]: https://github.com/dusk-network/piecrust/issues/324
[#301]: https://github.com/dusk-network/piecrust/issues/301
[#271]: https://github.com/dusk-network/piecrust/issues/271
[#268]: https://github.com/dusk-network/piecrust/issues/268
[#243]: https://github.com/dusk-network/piecrust/issues/243
[#222]: https://github.com/dusk-network/piecrust/issues/222
[#216]: https://github.com/dusk-network/piecrust/issues/216
[#209]: https://github.com/dusk-network/piecrust/issues/209
[#207]: https://github.com/dusk-network/piecrust/issues/207
[#201]: https://github.com/dusk-network/piecrust/issues/201
[#199]: https://github.com/dusk-network/piecrust/issues/199
[#192]: https://github.com/dusk-network/piecrust/issues/192
[#190]: https://github.com/dusk-network/piecrust/issues/190
[#189]: https://github.com/dusk-network/piecrust/issues/189
[#174]: https://github.com/dusk-network/piecrust/issues/174
[#157]: https://github.com/dusk-network/piecrust/issues/157
[#139]: https://github.com/dusk-network/piecrust/issues/139
[#136]: https://github.com/dusk-network/piecrust/issues/136

<!-- VERSIONS -->
[Unreleased]: https://github.com/dusk-network/piecrust/compare/uplink-0.16.0...HEAD
[0.16.0]: https://github.com/dusk-network/piecrust/compare/uplink-0.15.0...uplink-0.16.0
[0.15.0]: https://github.com/dusk-network/piecrust/compare/uplink-0.14.0...uplink-0.15.0
[0.14.0]: https://github.com/dusk-network/piecrust/compare/uplink-0.13.0...uplink-0.14.0
[0.13.0]: https://github.com/dusk-network/piecrust/compare/uplink-0.12.0...uplink-0.13.0
[0.12.0]: https://github.com/dusk-network/piecrust/compare/uplink-0.11.0...uplink-0.12.0
[0.11.0]: https://github.com/dusk-network/piecrust/compare/uplink-0.10.0...uplink-0.11.0
[0.10.0]: https://github.com/dusk-network/piecrust/compare/uplink-0.9.0...uplink-0.10.0
[0.9.0]: https://github.com/dusk-network/piecrust/compare/uplink-0.8.0...uplink-0.9.0
[0.8.0]: https://github.com/dusk-network/piecrust/compare/uplink-0.7.1...uplink-0.8.0
[0.7.1]: https://github.com/dusk-network/piecrust/compare/v0.7.0...uplink-0.7.1
[0.7.0]: https://github.com/dusk-network/piecrust/compare/uplink-0.6.1...v0.7.0
[0.6.1]: https://github.com/dusk-network/piecrust/compare/v0.6.0...uplink-0.6.1
[0.6.0]: https://github.com/dusk-network/piecrust/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/dusk-network/piecrust/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/dusk-network/piecrust/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/dusk-network/piecrust/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/dusk-network/piecrust/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/dusk-network/piecrust/releases/tag/v0.1.0
