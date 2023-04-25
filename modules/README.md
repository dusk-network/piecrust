# π-crust Modules

This workspace contains individual module examples. These examples demonstrate various functionalities and structures provided by the `piecrust` and `piecrust-uplink` libraries. 

## Module Examples

- [Box](box/): A module for managing a boxed i16 value with set and get operations.
- [Callcenter](callcenter/): Inter-module call example.
- [Constructor](constructor/): Module with a constructor.
- [Counter](counter/): Counter module that both reads and writes state.
- [Crossover](crossover/): Bi-directional inter-module call example.
- [Debugger](debugger/): Example of in-module debug calls.
- [Eventer](eventer/): Event emitting example.
- [Everest](everest/): Example of a module retrieving the block height from the host.
- [Fallible counter](fallible_counter/): Example of a counter that can panic if wanted.
- [Fibonacci](fibonacci/): Fibonacci and in-module recursion example.
- [Host](host/): Module that performs a simple host call.
- [Metadata](metadata/): Example of module metadata retrieval.
- [Micro](micro/): Minimal module example.
- [Spender](spender/): Module testing the gas spending behavior.
- [Stack](stack/): Simple nstack implementation.
- [Vector](vector/): Simple vector implementation.

## Dependencies

The examples in this workspace depend on the `piecrust-uplink` library.

## Testing

Tests for the module examples can be found in the `tests` folder in the `piecrust` crate.
