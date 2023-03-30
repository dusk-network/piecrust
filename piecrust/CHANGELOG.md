# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Derive `Debug` for `Session` and `VM`

### Fixed

- Removed SELF_ID export from contracts. [#167]
- Added uplink::owner and uplink::self_id. [#158]
- Added persistence fo module metadata. [#167]
- Added `DeployData` and `DeployDataBuilder`. [#158]
- Changed deploy API to accept `Into<DeployData>`. [#158]
- Added contract constructor mechanism. [#93]
- Added caching of module compilation outputs. [#162]
- Made modules compile at deploy time rather than on first query/transaction time. [#162]
- Removed errant print statements.

## [0.1.0] - 2023-03-15

- First `piecrust` release

<!-- ISSUES -->
[#93]: https://github.com/dusk-network/piecrust/issues/93
[#158]: https://github.com/dusk-network/piecrust/issues/158
[#162]: https://github.com/dusk-network/piecrust/issues/162
[#167]: https://github.com/dusk-network/piecrust/issues/167

<!-- VERSIONS -->
[Unreleased]: https://github.com/dusk-network/piecrust/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/dusk-network/piecrust/releases/tag/v0.1.0
