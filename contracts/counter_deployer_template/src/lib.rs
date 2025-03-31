// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Contract to act as a template for the counter deployer sample contract.

#![no_std]

extern crate alloc;

use piecrust_uplink as uplink;
use uplink::{ContractError, ContractId};
use alloc::string::ToString;
use alloc::vec::Vec;

/// Struct that describes the state of the Counter contract
pub struct Counter {
    value: i32,
}

impl Counter {
    pub fn init(&mut self, value: i32, fail: bool, fail_at: u32, additional_deploys: u32, owner: Vec<u8>) {
        if fail {
            panic!("Failed to deploy");
        }
        self.value = value;

        if additional_deploys > 0 {
            let deploy_nonce = additional_deploys as u64 + 100_000;
            let fail = fail_at == additional_deploys;
            let _ = uplink::call::<_, Result<ContractId, ContractError>>(
                ContractId::try_from("0101010101010101010101010101010101010101010101010101010101010101".to_string()).unwrap(),
                "recursive_deploy_through_init",
                &(value, fail, fail_at, additional_deploys - 1, deploy_nonce, owner.clone())
            );
        }
    }
}

/// State of the Counter contract
static mut STATE: Counter = Counter { value: 0 };

impl Counter {
    pub fn read_value(&self) -> i32 {
        self.value
    }

    pub fn increment(&mut self) {
        self.value += 1;
    }
}

/// Expose `Initializer::read_value()` to the host
#[no_mangle]
unsafe fn read_value(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |_: ()| STATE.read_value())
}

/// Expose `Initializer::increment()` to the host
#[no_mangle]
unsafe fn increment(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |_: ()| STATE.increment())
}

/// Expose `Initializer::init()` to the host
#[no_mangle]
unsafe fn init(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |(arg, fail, fail_at, additional_deploys, owner)| STATE.init(arg, fail, fail_at, additional_deploys, owner))
}
