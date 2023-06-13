# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Emit debug line on panic handler [#222]
- Add `debug` feature [#222]

### Changed

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
[#222]: https://github.com/dusk-network/piecrust/issues/222
[#216]: https://github.com/dusk-network/piecrust/issues/216
[#209]: https://github.com/dusk-network/piecrust/issues/209
[#207]: https://github.com/dusk-network/piecrust/issues/207
[#201]: https://github.com/dusk-network/piecrust/issues/201
[#199]: https://github.com/dusk-network/piecrust/issues/199
[#192]: https://github.com/dusk-network/piecrust/issues/192
[#174]: https://github.com/dusk-network/piecrust/issues/174
[#139]: https://github.com/dusk-network/piecrust/issues/139
[#136]: https://github.com/dusk-network/piecrust/issues/136

<!-- VERSIONS -->
[Unreleased]: https://github.com/dusk-network/piecrust/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/dusk-network/piecrust/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/dusk-network/piecrust/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/dusk-network/piecrust/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/dusk-network/piecrust/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/dusk-network/piecrust/releases/tag/v0.1.0
