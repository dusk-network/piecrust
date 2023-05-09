# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Add `call` and associated functions and structs [#136]
- Add `dlmalloc` feature [#199]
- Add crate-specific README [#174]

### Changed

- Change signature of `owner` to return `[u8; N]` instead of `[u8; 32]` [#201] 

### Removed

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
[#201]: https://github.com/dusk-network/piecrust/issues/201
[#199]: https://github.com/dusk-network/piecrust/issues/199
[#192]: https://github.com/dusk-network/piecrust/issues/192
[#174]: https://github.com/dusk-network/piecrust/issues/174
[#139]: https://github.com/dusk-network/piecrust/issues/139
[#136]: https://github.com/dusk-network/piecrust/issues/136

<!-- VERSIONS -->
[Unreleased]: https://github.com/dusk-network/piecrust/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/dusk-network/piecrust/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/dusk-network/piecrust/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/dusk-network/piecrust/releases/tag/v0.1.0
