# π-crust

[![Repository](https://img.shields.io/badge/github-piecrust-blueviolet?logo=github)](https://github.com/dusk-network/piecrust)
![Build Status](https://github.com/dusk-network/piecrust/workflows/build/badge.svg)
[![Documentation](https://img.shields.io/badge/docs-piecrust-blue?logo=rust)](https://docs.rs/piecrust/)

WASM virtual machine handling Dusk's smart contracts.

## Usage

```rust
use piecrust::VM;
let mut vm = VM::ephemeral().unwrap();

let bytecode = /*load bytecode*/;

let mut session = vm.genesis_session(SessionData::new());
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
