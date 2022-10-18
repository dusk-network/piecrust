# π-crust

[![Repository](https://img.shields.io/badge/github-piecrust-blueviolet?logo=github)](https://github.com/dusk-network/piecrust)
![Build Status](https://github.com/dusk-network/piecrust/workflows/build/badge.svg)
[![Documentation](https://img.shields.io/badge/docs-piecrust-blue?logo=rust)](https://docs.rs/piecrust/)

WASM virtual machine handling Dusk's smart contracts.

## Usage

```rust
use piecrust::{Error, VM};

let bytecode = // load module bytecode ;

let mut vm = VM::ephemeral()?;
let module_id = vm.deploy(bytecode)?;

let mut session = vm.session();
let result = session.transact::<i16, i32>(module_id, "function_name", 0x11)?;

// use result
```

## Build and Test

To build and test the crate one will need a
[Rust](https://www.rust-lang.org/tools/install) toolchain, Make, and the
`wasm-tools` binary.

```sh
sudo apt install -y make
cargo install wasm-tools
make test
```
