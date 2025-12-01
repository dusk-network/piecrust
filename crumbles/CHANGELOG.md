# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- Fix memory replacement on consecutive reverts

## [0.3.0] - 2023-10-11

### Added

- Add `LocateFile` trait for getting file paths for mapping
- Allow for choosing the size of the mapping

### Changed

- Mapping behavior is now lazy, mapping pages to their regions on demand
- Change `Mmap::with_files` to take `LocateFile` instead of `IntoIterator<Item = io::Result<(usize, File)>>`
- Change `Mmap::new` and `Mmap::with_files` to take `n_pages` and `page_size`

### Removed

- Remove `MEM_SIZE` and `PAGE_SIZE` constants

## [0.2.0] - 2023-09-13

### Changed

- Change `AsRef<[u8]>` and `AsMut<[u8]>` implementations for `Mmap` to always
  return the entire mapping
- Change segfault handler to no longer handle "Out of Bounds", since rust
  already handles this correctly - with a panic

### Removed

- Remove `Mmap::set_len`
- Remove `len` field from `MmapInner`

## [0.1.3] - 2023-09-11

### Fixed

- Fix files passed to `Mmap::from_file` never being closed

## [0.1.2] - 2023-09-07

### Added

- Add `Mmap::set_len` function

## [0.1.1] - 2023-09-07

### Fixed

- Fix memory protection on snapshotting functions

## [0.1.0] - 2023-08-30

### Added

- Initial release

<!-- ISSUES -->

<!-- VERSIONS -->
[Unreleased]: https://github.com/dusk-network/piecrust/compare/crumbles-0.3.0...HEAD
[0.3.0]: https://github.com/dusk-network/piecrust/compare/crumbles-0.2.0...crumbles-0.3.0
[0.2.0]: https://github.com/dusk-network/piecrust/compare/crumbles-0.1.3...crumbles-0.2.0
[0.1.3]: https://github.com/dusk-network/piecrust/compare/crumbles-0.1.2...crumbles-0.1.3
[0.1.2]: https://github.com/dusk-network/piecrust/compare/crumbles-0.1.1...crumbles-0.1.2
[0.1.1]: https://github.com/dusk-network/piecrust/compare/crumbles-0.1.0...crumbles-0.1.1
[0.1.0]: https://github.com/dusk-network/piecrust/releases/tag/crumbles-0.1.0
