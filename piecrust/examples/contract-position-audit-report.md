# Contract Merkle Position Audit Report

Date checked: 2026-07-01

This report records the latest-state contract Merkle position collision check
performed with `check_contract_positions.rs`.

## Checker

Rust example:

```text
piecrust/examples/check_contract_positions.rs
```

Command:

```sh
cargo run -p piecrust --example check_contract_positions -- \
  mainnet piecrust/examples/state-audit/mainnet/state \
  testnet piecrust/examples/state-audit/testnet/state
```

The checker scans `state/main/leaf/<contract-id>/element` entries and computes
each contract's folded Merkle position using the same Piecrust
`position_from_contract` implementation that backs contract state.

## State Snapshots Checked

| Network | Snapshot height checked | Local state path | Download URL |
| --- | ---: | --- | --- |
| mainnet | 4586973 | `piecrust/examples/state-audit/mainnet/state` | `https://nodes.dusk.network/state/4586973` |
| testnet | 3377981 | `piecrust/examples/state-audit/testnet/state` | `https://testnet.nodes.dusk.network/state/3377981` |

Archive hashes:

```text
mainnet state.tar.gz: 572655e5fb7b1b1fa7af68c27bc548a45c43ad83334d72c23acbe628e3642c7c
testnet state.tar.gz: 254b42d3a5d0deab4b67404cada1bcb4a9aa85527c97b11d41ade38448ade445
```

State IDs:

```text
mainnet: e5a4af3a62315f84ea6d9c033605510ec543c895ff9140bc5eb94620b8ee434c
testnet: c9d05b71de7ef69c6d50933d2cfa13263851819ac573e772d3311d2accccb4cd
```

## Piecrust Checker Output

```text
mainnet: contracts=8 unique_positions=8 collisions=0 skipped=0
testnet: contracts=265 unique_positions=265 collisions=0 skipped=0
```

Independent filesystem counts matched the checker input:

| Network | Contract leaf directories | `element` files |
| --- | ---: | ---: |
| mainnet | 8 | 8 |
| testnet | 265 | 265 |

## Live Chain Height Comparison

Live heights were queried from the wallet's default state endpoints with:

```graphql
query { blocks(last: 1) { header { height } } }
```

Default state endpoints:

```text
mainnet: https://nodes.dusk.network/graphql
testnet: https://testnet.nodes.dusk.network/graphql
```

| Network | Snapshot height checked | Live block height observed | Blocks behind |
| --- | ---: | ---: | ---: |
| mainnet | 4586973 | 4588766 | 1793 |
| testnet | 3377981 | 3681974 | 303993 |

## Conclusion

At the checked latest-state snapshots:

```text
mainnet block/state height 4586973: no contract-position collisions found
testnet block/state height 3377981: no contract-position collisions found
```

The checked mainnet state was 1793 blocks behind the observed mainnet tip.
The checked testnet state was 303993 blocks behind the observed testnet tip.
