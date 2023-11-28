

##Introduction

This tutorial assumes that you have a working Rust environment set up on your machine.
We also assume some minimal working knowledge of the Rust programming language.

Dusk blockchain runs smart contracts written in Web Assembly.
You can write a smart contract in any language that can be translated into Web Assembly,
as long as the resulting Web Assembly program conforms to a simple set of requirements.
This tutorial will focus on showing you how to write smart contracts for the Disk blockchain in Rust.

We will start with a very simple counter example, and then we will move on to describing the 
elements of that example. After that, we will generalize and show more complex mechanisms available for contracts
like inter-contract calls, host calls, persistence and more.
 
##Simple Counter Example

For the beginning, let's create a simple contract example for a counter. The counter will keep a count, and allow for
incrementing it by one, as well as for reading its current value. In other words, the counter contract will count the 
number of times its increment method has been called, and will make this count available via a read method.
As our contract is "almost" a normal Rusk program, let's create a new Rust cargo project in order to
be able to write it and compile it.

You can create new Rust library for our contract by issuing the following command:

`cargo new --lib hello-dusk-contract`

This command will create a small Rust library project in a folder named `hello-dusk-contract`.
You can open this project using your favorite IDE or with a simple system editor.
In folder `src` there is a file `lib.rs` with some sample test. Your can remove this generated content
by clearing up this file and then you can start entering the contract.

As our Rust contract will be translated to Web Assembly, we need to compile it without standard libraries,
as they won't be available at our Dusk blockchain runtime. Hence, first line of our contract will be:

`#![no_std]`

Next, in order to hook up methods of our contract as methods which are visible to our Dusk Virtual Machine
named PieCrust, we need to import it into our module via the standard Rust `use` declaration:

`use piecrust-uplink as uplink;`

Having this behind us, we can now focus on our counter functionality. Let's define our counter as a Rust
structure as follows:

```rust
pub struct Counter {
    value: i64,
}
```

Value of our counter will be kept as `value` field in a `Counter` struct.
As counter's value is our state, which needs to be preserved between contract methods' invocations,
we need to instantiate our state as a global static object:

```rust
static mut STATE: Counter = Counter { value: 0 };
```

Now we have our STATE of type Counter, but we also need methods to manipulate it. At this moment
we are at the realm of 'normal' Rust, there is nothing Dusk or blockchain-specific in the following code:

```rust
impl Counter {
    /// Increment the value of the counter by 1
    pub fn increment(&mut self) {
        self.value = self.value + 1;
    }
}
```

We also need a method to read the counter value, so eventually our Counter methods implementation block
will look as follows:

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

We created a Rust structure containing our state and two methods, one manipulating that state and the other
querying the state. Now we need to expose our methods to the Dusk Virtual Machine (PieCrust) so that 
it is able to "see" them and execute them when our contract is deployed. For this we need the following code:

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

And this is it, our contract is now ready. The `#[no_magle]` annotations are needed in order to turn off 
the default Rust linker name mangling - here we want our names to be as they are, since they will be called
via mechanisms outside of control of the linker. `uplink::wrap_call` takes care of all the boilerplate
code needed to serialize/deserialize and pass arguments to and from our methods.
As a result, our counter contract looks as follows:

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

When you do it, you encounter an error stating that a `piecrust-uplink` dependency is missing.
You need to enter the following line in the `[dependencies]` section of your Cargo.toml file, in order to alleviate
this error:

```toml
[dependencies]
piecrust-uplink = { version = "0.8", features = ["abi", "dlmalloc"] }
```

Now you should be able to compile successfully and after issuing a command:

`find . -name *.wasm`

you should be able to see a file named: `hello_dusk_contact.wasm`
That is the result of our compilation which can now be deployed on the Dusk Blockchain. 


##Contract State Persistence
After trying out and looking at the above example, you may wonder, how is it possible that counter state
is being persisted, although we did not do anything with the count value. Usually, smart contracts
provide persistence in a form of special key-value maps, accessible via an api provided by the contract host, i.e.,
its Virtual Machine. Here, we did not do anything to make the count persistent, yet it is being persistent
and when we try out the contract by subsequently calling increment and read_value, we can see that the count value
is correct. The answer to this question is that the entire memory of the contract gets persisted, along
with contract data. Hence, we don't need to do anything special to make data persistent. As the data is in memory,
it will be persisted along with the entire memory. A consequence of this is the fact, that you can use any data
structure or data collection in you program, and it will be persisted. You don't need to limit yourself to
a predefined set of types given to you by the blockchain runtime environment.

##Comparison with Rusk-VM Version 1.0
Now that you had a taste of how a counter example smart contract looks in PieCrust (a.k.a. Rusk-VM Version 2.0), 
it is worth to have a look at a functionally equivalent example written for Rusk-VM Version 1.0.
An example looks as follows:

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
As you can see, the code is much harder to understand and contains much more boilerplate code, including code
for parameters passing, their deserialization and return values serialization, special derivations of data
structures and more. You may wonder how was persistence implemented in Version 1.0, as there are no
special calls related to persistence in this version either. In Version 1.0, contract state is passed in its
entirety into each state changing method as an extra first parameter, similarly to how object-oriented languages
pass instance reference to method calls. Upon state alteration, a state changing method returns the new state as a
return value. State persistence is thus taken care of by the host.

##Eventing

In addition to querying or changing the contract state, a contract can also send events. Events are objects which hold
a topic string as well as an attched piece of data. Events are stored by the host and can be queried by the
caller of a method which generated events. If a contract has a need to inform a user about some operations 
or facts encountered during the execution, and this information may or may not be consumed by the user - events
are an ideal tool for that. Events have the advantage that they are not passed as return values of contracts.
Let's have a look at a small contract which generates events:

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
Method `emit_num` generates as many events as its argument tells it to. Events do not need to be passed
as return value, but rather are stored by the host and can optionally be queried later by the caller.
This is a very convenient mechanism for passing lightweight and optional information to the user.

##Feeder

Passing return value from contract query method via its return value is fine for relatively small values or data structures,
yet it is impractical for larger collections. The caller may want to process one collection element at a time, and for such
scenario a feeder mechanism can be used. Feeder passes data via a dedicated data channel called mpsc from Rust's standard library
(mpsc stands for multiple producer single consumer).
As contract writer, you do not need to worry about setting up a mpsc channel, as you can use a provided host method instead.
The following example shows a simple contract which utilizes a feeder:

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

Method `feed_num` in the above example uses the host call named `feed` in order to pass subsequent values of a collection (in this case simple
integers) to a communication channel. Caller of this method has a mechanism which allows it to pass a mpsc channel and consume values produced
by the contract.

##Host Functions
Contracts are "almost" regular Rust programs convertible to Web Assembly, which means, they do not use standard library, and do not use 
input/output functions. Contracts also run in a so called "hosted" environment, which means that they have some host services available
to them. Among those services there is a set of host functions they are allowed to call. Host functions are always available to contratcs, 
and so far we have encountered a few of them, like the following:
- wrap_call()
- emit()
- feed()

There are more host functions available and several of them will be described in this section. For the beginning, we'd like to mention the following 
host functions:
- owner()
- self_id()
- host_query()

First two of those methods belong to a group of so-called "metadata" methods, as they provide some information about the contract itself.
`owner()` provides contract id of contract's owner, while `self_id()` provides an id of a contract itself.
Sample contract utilizing these two host calls might look as follows:

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
As we can see, the host environment provides also some types, like in this example, `ContractId`.
The last of the host functions we'd like to mention in this section is `host_query()`.
`host_query()` is a universal function which allows contracts to call any function that was registered with the host
before the contract was called. Let's say that we would like to perform hashing on the host side. We could write
a hash function and register it with the host, so that subsequently we would be able to call it from
within a contract. Let's say our hashing function is as follows:

```rust
fn hash(buf: &mut [u8], len: u32) -> u32 {
    let a = unsafe { rkyv::archived_root::<Vec<u8>>(&buf[..len as usize]) };
    let v: Vec<u8> = a.deserialize(&mut rkyv::Infallible).unwrap();
    let hash = blake3::hash(&v);
    buf[..32].copy_from_slice(&hash.as_bytes()[..]);
    32
}
```

Our `hash` function deserializes passed vector of data, hashes it and places the returned hash in the same 
area where input parameter were passed. Return value is the length of a passed return data.

Function to be registered as host function needs to be of type `HostQuery`, which is defined as follows:
```rust
pub trait HostQuery: Send + Sync + Fn(&mut [u8], u32) -> u32 {}
```

Registration of a host function will look as follows:
```rust
vm.register_host_query("hash", hash);
```

After our hash function is registered, we are able to call it from withing a contract as follows:
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

##ZK Proof Verification
One of the host functions available for contracts is a function to verify Zero Knowledge proofs. A method withing a contract
performing a proof verification could look as follows:

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

In this way, contract is able to verify ZK proof without performing the verification by itself, but rather by delegating
the work to the host.

##Calling Other Contracts
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
Host function `call` makes it possible for the contract to call a method of a given contract (identified by its id). 
The function accepts contract id, name of the function to be called, as well as function argument, 
which in the above example is a unit type (argument is empty).
There is also another variant of the host `call()` function named `call_with_limit()`, which in addition
to contract id, method name and method argument, also accepts a limit value of gas to be spent by the given call.

##Inserting Debugging Statements
Contracts, being Web Assembly modules, running in a Virtual Machine sandbox, are not allowed to perform
any input/output operations. Sometimes it is needed, especially for debugging purposes, for the contract
to print a message on the console. For this purpose, a host macro named `debug!` has been provided.
In the following example, contract's method issues a debugging statement:

```rust
pub fn debug(&self, string: alloc::string::String) {
    uplink::debug!("Message from a smart contract: {}", string);
}
```

##Panicking
Sometimes it is necessary for a contract to panic, especially if some critical check of arguments or state
failed and there is no point to continue. Host macro named `panic!` is provided for this very purpose.
In the following example, contract's method panics:

```rust
pub fn check_funds(&self) {
    if self.funds <= 0 {
        uplink::panic!("Out of funds");
    }
}
```

