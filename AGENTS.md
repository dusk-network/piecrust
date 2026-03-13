# Piecrust

Piecrust is Dusk's WASM smart contract virtual machine. It manages contract deployment, execution, state storage, and gas metering.

## Repository Map

```
piecrust/
├── crumbles/          # Copy-on-write memory-mapped regions with dirty page tracking
├── piecrust-uplink/   # ABI/SDK for writing smart contracts targeting piecrust
├── piecrust/          # VM core — session management, WASM execution, state storage
├── contracts/         # Example smart contracts used in tests
└── Makefile           # Root Makefile delegating to member crates
```

## Commands

```bash
make test              # Full suite: contracts build, cold-reboot, size assert, then crumbles + uplink + piecrust tests
make -C <crate> test   # Single crate tests
make clippy            # All crates (warnings = errors)
make fmt               # Format (requires nightly)
make no-std            # Verify piecrust-uplink compiles to wasm32
make setup-compiler    # Install dusk compiler (required for wasm64 builds)
```

## Feature Flags

| Flag | Crate | Description | Default |
|------|-------|-------------|---------|
| `debug` | `piecrust` | Required for most integration tests | No |
| `call-hook` | `piecrust` | Inter-contract call observation/rejection | No |
| `serde` | `piecrust-uplink` | Serde support for uplink types | No |

`piecrust-uplink` has a default panic handler that conflicts with `std`. Do not use `--all-features` with uplink — use `--no-default-features` instead.

## Architecture

- Uses `dusk-wasmtime` (alpha fork) as WASM runtime
- Session-based state mutations with copy-on-write memory (via crumbles)
- Gas metering via compiler middleware
- Commit/rollback semantics for state changes
- Contract-host communication via shared argument buffer (piecrust-uplink)
- Supports both wasm32 and wasm64 targets

## Elevated Care Zones

- **State storage** (`piecrust/src/store/`): commit/session semantics, merkle tree integrity — data corruption or ordering bugs break consensus
- **WASM imports** (`piecrust/src/imports.rs`): host <-> contract boundary, argument buffer, gas metering — security-critical
- **Copy-on-write memory** (`crumbles/`): low-level mmap and dirty page tracking — page alignment and platform differences matter

## Conventions

- **`no_std`**: `contracts/` and `piecrust-uplink` — don't add `std` imports
- **Serialization**: `rkyv` types are compatibility boundaries — don't reorder fields
- **Errors**: return `Result` — no `unwrap()`/`expect()` outside tests
- **Logging**: `tracing` macros only, never `println!`
- **Clippy**: don't suppress warnings — fix the underlying issue

## Change Propagation

| Changed crate | Also verify |
|---------------|-------------|
| `crumbles` | `piecrust` |
| `piecrust-uplink` | `piecrust`, `rusk/contracts` |
| `piecrust` | `rusk/vm`, `rusk/contracts` |

## Git Conventions

- Default branch: `main`
- License: MPL-2.0

### Commit messages

Format: `<scope>: <Description>` — imperative mood, capitalize first word after colon.

**One commit per crate per concern.** Each commit touches exactly one crate and one logical concern. Never bundle changes to different crates in one commit, and don't mix unrelated changes within the same crate either. Order commits bottom-up through the dependency chain (`crumbles` → `piecrust-uplink` → `piecrust`).

Canonical scopes:

| Scope | Directory |
|-------|-----------|
| `crumbles` | `crumbles/` |
| `piecrust-uplink` | `piecrust-uplink/` |
| `piecrust` | `piecrust/` |
| `contracts` | `contracts/` |
| `workspace` | Root `Cargo.toml`, Makefile |
| `ci` | `.github/workflows/` |
| `docs` | Documentation-only changes |

Examples:
- `piecrust: Add module cache for compiled WASM`
- `piecrust-uplink: Expose new host query ABI`
- `crumbles: Fix dirty page tracking on resize`
- `workspace: Update dusk dependencies`

### Changelog

Every PR that changes crate behavior must include a CHANGELOG.md entry. Each modified crate with a `CHANGELOG.md` gets an entry under `## [Unreleased]` using keep-a-changelog subsections (`### Added`, `### Changed`, `### Fixed`, `### Removed`). One bullet per logical change. If the work traces to a GitHub issue, reference it as a link: `[#123](https://github.com/dusk-network/piecrust/issues/123)`. Pure formatting, CI, docs-only, or internal refactors with no behavior change don't need entries.
