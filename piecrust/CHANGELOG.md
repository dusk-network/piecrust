# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Add crate-specific README. [#174]

## [0.3.0] - 2023-04-26

### Changed

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
[#181]: https://github.com/dusk-network/piecrust/issues/181
[#178]: https://github.com/dusk-network/piecrust/issues/178
[#174]: https://github.com/dusk-network/piecrust/issues/174
[#167]: https://github.com/dusk-network/piecrust/issues/167
[#162]: https://github.com/dusk-network/piecrust/issues/162
[#158]: https://github.com/dusk-network/piecrust/issues/158
[#93]: https://github.com/dusk-network/piecrust/issues/93

<!-- VERSIONS -->
[Unreleased]: https://github.com/dusk-network/piecrust/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/dusk-network/piecrust/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/dusk-network/piecrust/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/dusk-network/piecrust/releases/tag/v0.1.0
