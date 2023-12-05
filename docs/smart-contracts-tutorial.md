# Dusk VM and Smart Contracts

## Preface
This tutorial will focus on showing you how to write smart contracts for the Dusk blockchain in Rust. It focuses on contracts themselves and does not explain how to call the contracts from other tools and it does not explain how to deploy a contract. This tutorial focuses on contract writing. To follow this tutorial it is recommended that you have a working Rust environment set up on your machine, so that you can paste and compile the samples provided. We assume reader's moderate working knowledge of the Rust programming language.

## Introduction
Dusk blockchain runs smart contracts compiled into Web Assembly, an open-standard, portable binary-code format. Therefore, you can write a smart contract in any language that can be translated into Web Assembly, such as Rust, C, C++, Go and other, as long as the resulting Web Assembly program conforms to a simple set of requirements.

Smart contracts are executed by the Dusk Virtual Machine which handles contract's bytecode, its state, its runtime sanboxed environment, in order to execute contract's methods (by methods we mean the functions which contracts make externally available by exporting, to be called by external paries or other contracts).
There are two kinds of methods:
- queries
- transactions

Queries do not change contract's state, yet they are able to return data, either state data or some other data calculated by them. Transactions, on the other hand, do change contract's state, yet are not able to return data.

At this point it is important to understand the concept of contract's state. By contract's state we mean any data that is kept by contract in a persistent manner. As we are in a context of blockchain, our state is global (it is what we call a global singleton). Given contract has only a single state globally at any given time, and this state is maintained by all nodes operating the Dusk blockchain. A smart contract has access to its state and can provide some values of it or the entire state via queries. On the other hand, contract's transactions allow the state to be mutated. As state lives in a distributed ledger and its history is immutable, transactions make the history move forward. After executing a transaction we obtain a new state, state history is advancing one step ahead. All previous states are preserved.
As far as queries are concerned, there are two ways queries can return values to the caller:
- via the return value
- via the feeder

Returning a value is similar to a regular programming language function returning a value. In case of Dusk VM contracts, we have an argument passing buffer which will contain data returned, and we also pass back returned data length. For collections and larger size data such way of passing return value is less useful, and in such cases we may utilize a feeder. Feeder is based on Rust `mcsp` (mutiple producer single consumer) channel, and allows query caller to successively consume data passed to the feeder by contract's method. More information about feeder will be provided in a section below.

Calling smart contracts' queries and transaction methods is not free. Caller of contracts' methods needs to pay gas in order for methods to be executed. As we are not focusing in this tutorial on the caller's side, we do not deal with gas here, except when we discuss inter-contract calls. Nevertheless, you as a smart contract writer need to be aware that execution of contract's every instruction costs real money and that you need to conserve spent computational power as much as possible. That is why it is important to understand well the concept of hosting and host methods which are provided for the contracts. Smart contracts run in a sandboxed environment which is started and controlled by the host. Host provides a set of methods to be called by contracts, some of which can be used to significantly conserve computational power spent by contracts. Performing the same function on host is always cheaper than performing it by contract's code. Hence, if some functionality on the host side is available, it should be used rather than duplicated by contract's code. This applies especially to cryptographic and ZK-related functions, which are computationally intense. You are encouraged to get familiar with methods provided by the host and to use them.

Contracts encompass transactions, which mutate state, and queries, which provide return values either directly or via a feeder. There is one more mechanism which can be used to obtain lightweight feedback information from contracts, this mechanism is eventing. Events can be emitted by both queries and transactions, and they can be processed by query or transaction caller after the call is finished. Events are very useful for triggering some actions on caller's side. In this tutorial we will cover only sending events, as receiving and processing events belongs to the calling side of the smart contracts' domain.

While writing contracts, it is beneficial to be aware how parameters are passed back and forth to and from queries and to transactions. It is not critical to know a lot of details of this mechanism, as the details are conveniently hidden from us by the very useful host methods provided, yet it is good to have a general idea. Every smart contract, when deployed and run, has an argument passing area in memory, called `A` (at the time of writing the size of A is one page, which is 64kB). When calling query or transaction, i.e., a function exported by the smart contract, arguments are serialized by the calling side and the result of serialization is placed in the buffer `A`. Once arguments are in the buffer, the call to query or transaction is being made, and the only actual argument to the smart contract function called is the length of the data in the buffer `A`. In other words, only a 32 bit number is actually passed to the function, while the real function argument is in buffer A. The same happens upon return from a query; what is actually being passed is the length of the data in buffer `A`, while the real return value is stored in buffer `A`. Contract's method, when receiving an argument, reads it from buffer `A` and deserializes it, knowing its length from the parameter passed. This technique is hidden from the contract writer, as host method `wrap_call` takes care of all the details. Should you ever wonder about a strange signature of functions declared under the `#[no_magle]` annotation, this is the explanation for it.

We will start with a very simple counter example, and then we will move on to describing the elements of that example. After that, we will generalize and show more complex mechanisms available for contracts like inter-contract calls, host calls, persistence and more.

## Simple Counter Example

For the beginning, let's create a simple contract example for a counter. The counter will keep a count, and allow for incrementing it by one, as well as for reading its current value. In other words, the counter contract will count the number of times its increment method has been called, and will make this count available via a read method. As our contract (as any Dusk VM contract) is "almost" a normal Rusk program, let's create a new Rust cargo project in order to be able to write it and compile it.

You can create new Rust library for our contract by issuing the following command:

`cargo new --lib hello-dusk-contract`

This command will create a small Rust library project in a folder named `hello-dusk-contract`. You can open files of this project using your favorite IDE or with a simple system editor. In folder `src` there is a file `lib.rs` with some sample test. Your can remove this generated content by clearing up the file and then you can start entering or pasting in the contract's code.

As our Rust contract will be translated into Web Assembly, we need to compile it without standard libraries, as they won't be available at our Dusk VM runtime. Hence, first line of our contract will be:

`#![no_std]`

Next, in order to hook up methods of our Dusk VM host, we need to import a special Dusk VM library into our module via the standard Rust `use` declaration:

`use piecrust-uplink as uplink;`

We can now focus on our counter functionality. Let's define our counter state as a Rust structure as follows:

```rust
pub struct Counter {
    value: i64,
}
```

Value of our counter will be kept as a `value` field in `Counter` struct. As counter's value is our state, which needs to be preserved between contract methods' invocations, we need to instantiate our state as global static object:

```rust
static mut STATE: Counter = Counter { value: 0 };
```

Now we have our STATE of type Counter, but we also need methods to manipulate it. At this moment we are in the realm of 'normal' Rust, there is nothing Dusk VM or blockchain-specific in the following code:

```rust
impl Counter {
    pub fn increment(&mut self) {
        self.value += 1;
    }
}
```

We also need a method to read the counter value, so eventually our Counter methods implementation block will look as follows:

```rust
impl Counter {
    /// Read the value of the counter
    pub fn read_value(&self) -> i64 {
        self.value
    }

    /// Increment the value of the counter by 1
    pub fn increment(&mut self) {
        let value += 1;
        self.value = value;
    }
}
```

We created a Rust structure containing our state and two methods, one manipulating the state and the other querying the state's value. Now we need to expose our methods to the Dusk Virtual Machine so that it is able to "see" them and execute them after our contract is deployed. For this we need the following code:

```rust
#[no_mangle]
unsafe fn increment(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |_: ()| STATE.increment())
}
```

Similarly, to expose the `read_value` method we need to following code:
```rust
#[no_mangle]
unsafe fn read_value(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |_: ()| STATE.read_value())
}
```

Our contract is now ready. The `#[no_magle]` annotations are needed in order to turn off the default Rust linker name mangling - here we want our names to be as they are, since they will be called via mechanisms outside of control of the linker. `uplink::wrap_call` takes care of all the boilerplate code needed to serialize/deserialize and pass arguments to and from our methods. As a result, our entire counter contract looks as follows:

```rust
#![no_std]

use piecrust_uplink as uplink;

pub struct Counter {
    value: i64,
}

static mut STATE: Counter = Counter { value: 0xfc };

impl Counter {
    pub fn read_value(&self) -> i64 {
        self.value
    }

    pub fn increment(&mut self) {
        self.value += 1;
    }
}

#[no_mangle]
unsafe fn read_value(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |_: ()| STATE.read_value())
}

#[no_mangle]
unsafe fn increment(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |_: ()| STATE.increment())
}
```

You can now issue the following command and see if it compiles:

`cargo build --release --target wams32-unknown-unknown`

When you do it, you encounter an error stating that a `piecrust-uplink` dependency is missing. In order to alleviate this error, you need to enter the following line in the `[dependencies]` section of your Cargo.toml file:

```toml
[dependencies]
piecrust-uplink = { version = "0.8", features = ["abi", "dlmalloc"] }
```

Now you should be able to compile successfully and after issuing a command:

`find . -name *.wasm`

you should be able to see a file named: `hello_dusk_contact.wasm` That is the result of our compilation which can now be deployed to the Dusk Blockchain.

## Contract State Persistence
After trying out the above example, you may wonder, how is it possible that counter state is being persisted, although we did not do anything special with the count value. Usually, smart contracts provide persistence in a form of special key-value maps, accessible via an api provided by the contract host. Here, we did not do anything to make the count persistent, yet it is being persistent and when we try out the contract by subsequently calling increment and read_value, we can see that the count value  is correct. The answer is that the entire memory of a contract gets persisted, along with contract data. Thus, we don't need to do anything special to make data persistent. As data is in memory, it will be persisted along with the entire memory. A consequence of this is the fact that you can use any data structure or data collection in you program, and it will be persisted. You don't need to limit yourself to a predefined set of types provided to you by the blockchain's VM runtime environment.

## Comparison with Rusk-VM Version 1.0
Now that you had a taste of how a counter example smart contract looks in Rusk-VM Version 2.0, it is worth to have a look at a functionally equivalent example written for Rusk-VM Version 1.0. An example looks as follows:

```rust
#![cfg_attr(target_arch = "wasm32", no_std)]
use canonical_derive::Canon;

pub const READ_VALUE: u8 = 0;
pub const INCREMENT: u8 = 0;

#[derive(Clone, Canon, Debug)]
pub struct Counter {
    value: i32,
}

impl Counter {
    pub fn new(value: i32) -> Self {
        Counter {
            value,
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod hosted {
    use super::*;

    use canonical::{Canon, CanonError, Sink, Source};
    use dusk_abi::{ContractState, ReturnValue};

    const PAGE_SIZE: usize = 1024 * 4;

    impl Counter {
        pub fn read_value(&self) -> i32 {
            self.value
        }

        pub fn increment(&mut self) {
            self.value += 1;
        }
    }

    fn query(bytes: &mut [u8; PAGE_SIZE]) -> Result<(), CanonError> {
        let mut source = Source::new(&bytes[..]);

        let slf = Counter::decode(&mut source)?;

        let qid = u8::decode(&mut source)?;
        match qid {
            READ_VALUE => {
                let ret = slf.read_value();
                let mut sink = Sink::new(&mut bytes[..]);
                ReturnValue::from_canon(&ret).encode(&mut sink);
                Ok(())
            }
            _ => panic!(""),
        }
    }

    #[no_mangle]
    fn q(bytes: &mut [u8; PAGE_SIZE]) {
        let _ = query(bytes);
    }

    fn transaction(bytes: &mut [u8; PAGE_SIZE]) -> Result<(), CanonError> {
        let mut source = Source::new(bytes);

        let mut slf = Counter::decode(&mut source)?;
        let tid = u8::decode(&mut source)?;
        match tid {
            INCREMENT => {
                slf.increment();
                let mut sink = Sink::new(&mut bytes[..]);
                ContractState::from_canon(&slf).encode(&mut sink);
                ReturnValue::from_canon(&()).encode(&mut sink);
                Ok(())
            }
            _ => panic!(""),
        }
    }

    #[no_mangle]
    fn t(bytes: &mut [u8; PAGE_SIZE]) {
        transaction(bytes).unwrap()
    }
}
```
As you can see, the sample is much harder to follow and contains much more boilerplate code, including code for arguments passing, deserialization, serialization of return values, special derivations for data structures and more. You may wonder how was persistence implemented in Version 1.0, as there are no special calls related to persistence in this version either. In Version 1.0, contract state is passed in its entirety into each state changing method as an extra first parameter, in a similar manner to how object-oriented languages pass instance references to method calls. After state alteration, a state changing method returns the new state as a return value. State persistence is thus taken care of by the host. You can appreciate that that was a more heavy-weight and less performant solution.

## Eventing

In addition to querying and changing the contract state, Rusk VM smart contract can also send events. Events are (by intention light-weight) objects which hold a topic string as well as an attched piece of data. Events are stored by the host and can be queried by the caller of a method which generated events. If contract has a need to inform a user about some operations or facts encountered during execution, and this information may or may not be consumed by the user - events are an ideal tool for that. Events have the advantage that they are not passed as return values of contracts, and because of that many events can be sent during a single query or transaction execution. Let's have a look at a small contract which generates events:

```rust
#![no_std]

use piecrust_uplink as uplink;

pub struct Eventer;

static mut STATE: Eventer = Eventer;

impl Eventer {
    pub fn emit_num(&mut self, num: u32) {
        for i in 0..num {
            uplink::emit("number", i);
        }
    }
}

#[no_mangle]
unsafe fn emit_events(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |num| STATE.emit_num(num))
}
```
Method `emit_num` of the above contract generates a number of events, according to the value of its argument. Events do not need to be passed as return value, but rather are stored by the host and can optionally be queried later by the caller. This is a very convenient mechanism for passing lightweight and optional information to the user, and for triggering some actions on the user side.

## Feeder

Passing return value from a contract query method via its return value is fine for relatively small values or data structures, yet it is impractical for larger collections. The caller may want to process one collection element at a time, and for such scenario a feeder mechanism can be used and it is usually a better alternative. Feeder passes data via a dedicated data channel called `mpsc` from Rust's standard library (mpsc stands for multiple producer single consumer). As contract writer, you do not need to worry about setting up a mpsc channel, as you can use a provided host method instead. The following example shows a simple contract which utilizes a feeder:

```rust
#![no_std]

use piecrust_uplink as uplink;

pub struct Feeder;
static mut STATE: Feeder = Feeder;

impl Feeder {
    pub fn feed_num(&self, num: u32) {
        for i in 0..num {
            uplink::feed(i);
        }
    }
}

#[no_mangle]
unsafe fn feed_num(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |num| STATE.feed_num(num))
}
```

Method `feed_num` in the above example uses the host call named `feed` in order to pass subsequent values of a collection (in this case simple integers) to a `mpsc` communication channel. Caller of this method has a mechanism which allows it to pass an `mpsc` channel and can consume values as they arrive from the contract.

## Host Functions
Contracts are "almost" regular Rust programs convertible to Web Assembly, which means that they follow the usual non-VM requirements like not using the standard library and not using input/output functions. Contracts also run in a so called "hosted" environment, which means that they have some host services available to them. Among those services there is a set of host functions they are allowed to call. Host functions are always available to contracts. So far we have encountered a few of them, like the following:
- wrap_call()
- emit()
- feed()

There are more host functions available and some of them will be described in this section. For the beginning, we'd like to mention the following host functions:
- owner()
- self_id()
- host_query()

First two of those methods belong to a group of so-called "metadata" methods, named so because they provide some information about the contract itself. Method `owner()` provides contract id of the contract's owner, while method `self_id()` provides id of the contract itself. Sample contract utilizing these two host calls might look as follows:

```rust
#![no_std]

use piecrust_uplink as uplink;
use uplink::ContractId;

pub struct Metadata;
static mut STATE: Metadata = Metadata;

impl Metadata {
    pub fn read_owner(&self) -> [u8; 33] {
        uplink::owner()
    }
    pub fn read_id(&self) -> ContractId {
        uplink::self_id()
    }
}

#[no_mangle]
unsafe fn read_owner(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |_: ()| STATE.read_owner())
}
#[no_mangle]
unsafe fn read_id(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |_: ()| STATE.read_id())
}
```
As we can see, the host environment provides also some types, like in this example, `ContractId`. The last of the host methods we'd like to mention in this section is `host_query()`. Method `host_query()` is a universal function which allows contracts to call any function that was registered with the host before the contract was called. Let's say that we would like to perform hashing on the host side rather than by contract's code. We can write a hash function and register it with the host, so that subsequently we are able to call it from within a contract. Let's say our hashing function is as follows:

```rust
fn hash(buf: &mut [u8], len: u32) -> u32 {
    let a = unsafe { rkyv::archived_root::<Vec<u8>>(&buf[..len as usize]) };
    let v: Vec<u8> = a.deserialize(&mut rkyv::Infallible).unwrap();
    let hash = blake3::hash(&v);
    buf[..32].copy_from_slice(&hash.as_bytes()[..]);
    32
}
```

Our `hash` function deserializes an argument in a form of a vector of data, hashes it and places the hash in the same area where input parameter were passed. Return value is the length of a passed return data. Here we can see how much harder it is to write code when helpful host methods like `wrap_call` are not available.

Function to be registered as host function needs to be of type `HostQuery`, which is defined as follows:
```rust
pub trait HostQuery: Send + Sync + Fn(&mut [u8], u32) -> u32 {}
```

Registration of a host function looks as follows:
```rust
vm.register_host_query("hash", hash);
```

After our hash function is registered, we are able to call it from within a contract as follows:
```rust
#![no_std]

use alloc::vec::Vec;

use piecrust_uplink as uplink;

pub struct Hoster;
static mut STATE: Hoster = Hoster;

impl Hoster {
    pub fn host_hash(&self, bytes: Vec<u8>) -> [u8; 32] {
        uplink::host_query("hash", bytes)
    }
}

#[no_mangle]
unsafe fn host_hash(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |num| STATE.host_hash(num))
}
```

## ZK Proof Verification
One of the host functions available for contracts is a function to verify Zero Knowledge (ZK) proofs. A method within a contract performing a proof verification could look as follows:

```rust
    fn assert_proof(
        verifier_data: &[u8],
        proof: &[u8],
        public_inputs: &[PublicInput],
    ) -> Result<(), Error> {
        rusk_abi::verify_proof(verifier_data, proof, public_inputs)
            .then_some(())
            .ok_or(Error::ProofVerification)
    }
```

In this way, contract is able to verify ZK proof without having to perform the verification itself, but rather by delegating
the work to the host.

## Calling Other Contracts
Contracts are allowed to call other contracts, as in the following example:

```rust
#![no_std]

use piecrust_uplink as uplink;
use uplink::ContractId;

pub struct Callcenter;
static mut STATE: Callcenter = Callcenter;

impl Callcenter {
    pub fn increment_counter(&mut self, counter_id: ContractId) {
        uplink::call(counter_id, "increment", &()).unwrap()
    }
}

#[no_mangle]
unsafe fn increment_counter(arg_len: u32) -> u32 {
    wrap_call(arg_len, |counter_id| STATE.increment_counter(counter_id))
}
```
Host method `call` makes it possible for the contract to call a method of a given another contract (identified by its id). The function accepts contract id, name of the function to be called, and function argument, which in the above example is a unit type (argument is empty). There is also another variant of the host `call()` function named `call_with_limit()`, which in addition to contract id, method name and method argument, accepts a maximum value of gas to be spent by the given call.

## Inserting Debugging Statements
Contracts, being Web Assembly modules, running in a Virtual Machine sandbox, are not allowed to perform any input/output operations. Sometimes it is needed, especially for debugging purposes, for the contract to print a message on the console. For this purpose, a host macro named `debug!` has been provided. In the following example, contract's method issues a debugging statement:

```rust
pub fn debug(&self, string: alloc::string::String) {
    uplink::debug!("Message from a smart contract: {}", string);
}
```

## Panicking
Sometimes it is necessary for a contract to panic, especially if some critical check of arguments or state fails and there is no point for the contract to continue its execution and waste valuable resources. Host macro named `panic!` is provided for this very purpose. In the following example, contract's method panics:

```rust
pub fn check_funds(&self) {
    if self.funds <= 0 {
        uplink::panic!("Out of funds");
    }
}
```

## Constructor and Init
It is possible to export a special contract method named `init()` which can perform contact's initialization of any kind. Such method will be called automatically when the contract is deployed. The main intention behind method `init()` is to allow contracts to initialize their state at a time before the contract is operational and ready to receive calls. Method `init()` accepts a single argument of any serializable type. That argument will be passed to the init method by code which performs the deployment of the contract. In the following example, we can see a contract with an `init()` method:

```rust
#![no_std]
use piecrust_uplink as uplink;

pub struct State {
    value: u64,
}

impl State {
    pub fn init(&mut self, value: u64) {
        self.value = value;
    }
}

static mut STATE: State = State{ value: 0u64 };

impl State {
    pub fn read_value(&self) -> u64 {
        self.value
    }
}

#[no_mangle]
unsafe fn read_value(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |_: ()| STATE.read_value())
}

#[no_mangle]
unsafe fn init(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |arg: u8| STATE.init(arg))
}
```

Method `init()` looks like any contract method, and it could do anything other methods can do, it is not limited to only initializing contract's state. What is special about this method is the fact that the host will detect if it is exported, and it will call it when when the contract is deployed. Let's have a look at how the deployment of the such contract could be implemented:

```rust
fn deploy_contract_with_constrtor() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    const OWNER: [u8; 32] = [7u8; 32];

    let mut session = vm.session(SessionData::builder())?;

    let id = session.deploy(
        contract_bytecode!("constructor_example_contract"),
        ContractData::builder(OWNER).constructor_arg(&0xcafeu64),
        LIMIT,
    )?;
}
```
As we can see, method `deploy()` accepts an argument of type `Into<ContractData>`, so any object convertible to ContractData will be accepted. ContractData, on the other hand, contains a field named `constuctor_arg`, which is optional, but when set, will be used as an argument to the `init()` method of our contract. In effect, we are able to pass data from deployment code, like a contract deployment tool or a wallet, to contract state. Note that in the above example obligatory argument `owner` also had to be provided.
