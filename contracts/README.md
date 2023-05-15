# π-crust Contracts

This workspace contains individual contract examples. These examples demonstrate various functionalities and structures provided by the `piecrust` and `piecrust-uplink` libraries. 

## Contract Examples

- [Box](box/): A contract for managing a boxed i16 value with set and get operations.
- [Callcenter](callcenter/): Inter-contract call example.
- [Constructor](constructor/): Contract with a constructor.
- [Counter](counter/): Counter contract that both reads and writes state.
- [Crossover](crossover/): Bi-directional inter-contract call example.
- [Debugger](debugger/): Example of in-contract debug calls.
- [Eventer](eventer/): Event emitting example.
- [Everest](everest/): Example of a contract retrieving the block height from the host.
- [Fallible counter](fallible_counter/): Example of a counter that can panic if wanted.
- [Fibonacci](fibonacci/): Fibonacci and in-contract recursion example.
- [Host](host/): Contract that performs a simple host call.
- [Merkle](merkle/): A Merkle tree in an example contract.
- [Metadata](metadata/): Example of contract metadata retrieval.
- [Micro](micro/): Minimal contract example.
- [Spender](spender/): Contract testing the gas spending behavior.
- [Stack](stack/): Simple nstack implementation.
- [Vector](vector/): Simple vector implementation.

## Dependencies

The examples in this workspace depend on the `piecrust-uplink` library.

## Testing

Tests for the contract examples can be found in the `tests` folder in the `piecrust` crate.
