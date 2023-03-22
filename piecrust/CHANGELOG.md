# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Derive `Debug` for `Session` and `VM`

### Fixed

- Added contract constructor mechanism [#93]
- Added caching of module compilation outputs [#162]
- Made modules compile at deploy time rather than on first query/transaction time [#162]
- Removed errant print statements

## [0.1.0] - 2023-03-15

- First `piecrust` release

<!-- ISSUES -->
[#93]: https://github.com/dusk-network/piecrust/issues/93
[#162]: https://github.com/dusk-network/piecrust/issues/162

<!-- VERSIONS -->
[Unreleased]: https://github.com/dusk-network/piecrust/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/dusk-network/piecrust/releases/tag/v0.1.0
