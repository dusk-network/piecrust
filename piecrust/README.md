# π-crust

[![Repository](https://img.shields.io/badge/github-piecrust-blueviolet?logo=github)](https://github.com/dusk-network/piecrust)
![Build Status](https://github.com/dusk-network/piecrust/workflows/build/badge.svg)
[![Documentation](https://img.shields.io/badge/docs-piecrust-blue?logo=rust)](https://docs.rs/piecrust/)

WASM virtual machine for running Dusk's smart contracts.

## Usage

```rust
use piecrust::VM;
let mut vm = VM::ephemeral().unwrap();

let bytecode = /*load bytecode*/;

let mut session = vm.session(SessionData::builder())?;
let contract_id = session.deploy(bytecode).unwrap();

let result = session.transact::<i16, i32>(contract_id, "function_name", &0x11)?;

// use result
```

## Build and Test

To build and test the crate one will need a
[Rust](https://www.rust-lang.org/tools/install) toolchain, Make, and the
`wasm-tools` binary.

```sh
sudo apt install -y make # ubuntu/debian - adapt to own system
cargo install wasm-tools
make test
```

## Release History

To see the release history for this crate, please see the [CHANGELOG](./CHANGELOG.md) file.

## License

This code is licensed under the Mozilla Public License Version 2.0 (MPL-2.0). Please see the [LICENSE](./LICENSE) for further details.

## Contribute

If you want to contribute to this project, please check the [CONTRIBUTING](https://github.com/dusk-network/.github/blob/main/.github/CONTRIBUTING.md) file.
