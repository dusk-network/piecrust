# π-crust Uplink

[![Repository](https://img.shields.io/badge/github-piecrust-blueviolet?logo=github)](https://github.com/dusk-network/piecrust)
![Build Status](https://github.com/dusk-network/piecrust/workflows/build/badge.svg)
[![Documentation](https://img.shields.io/badge/docs-piecrust-blue?logo=rust)](https://docs.rs/piecrust-uplink/)

Piecrust Uplink is the library that allows you to build smart contracts directly on top of Dusk's Piecrust virtual machine. 

## Usage

The library allows users of the contract platform to manage the interface and state with the host environment of the modules. The example below describes a barebones module. For more detailed examples, see the [modules](https://github.com/dusk-network/piecrust/tree/main/modules) folder.

Add `piecrust_uplink` as a dependency to your module project:
```sh
cargo install piecrust_uplink
```

To make use of `uplink`, import the dependency in your project and mark it as `no_std`:
```rust
#![no_std]

use piecrust_uplink as uplink;
use uplink::State;
```

To attach state to a contract:
```rust
/// Struct that describe the state for your module
pub struct Counter {
    value: i64,
};

/// State of the module
static mut STATE: State<Counter> = State::new(Counter { value: 0x1 });
```

To define logic for your module, define an implementation:
```rust
impl Counter {
    pub fn read_value(&self) -> i64 {
        self.value
    }

    pub fn increment(&mut self) {
        let value = self.value + 1;
    }
}
```

Read and write operations need to be exposed to the host. Add the following below the implementation:
```rust
unsafe fn read_value(arg_len: u32) -> u32 {
    uplink::wrap_query(arg_len, |_: ()| STATE.read_value())
}

#[no_mangle]
unsafe fn increment(arg_len: u32) -> u32 {
    uplink::wrap_transaction(arg_len, |panic: bool| STATE.increment(panic))
}
```

Note how a read operation is using `wrap_query`, while a state mutation uses `wrap_transaction.

## Release History

To see the release history for this crate, please see the [CHANGELOG](./CHANGELOG.md) file.

## License

This code is licensed under the Mozilla Public License Version 2.0 (MPL-2.0). Please see the [LICENSE](./LICENSE) for further details.

## Contribute

If you want to contribute to this project, please check the [CONTRIBUTING](https://github.com/dusk-network/.github/blob/main/.github/CONTRIBUTING.md) file.
