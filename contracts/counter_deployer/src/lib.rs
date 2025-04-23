// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Contract to test the contract deploy functionality.

#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use core::convert::TryInto;

use piecrust_uplink::{self as uplink, ContractError};
use uplink::ContractId;

/// Struct that describes the state of the counter deployer contract
pub struct CounterDeployer<'a> {
    bytecode: &'a [u8],
}

/// State of the counter deployer contract
static mut STATE: CounterDeployer = CounterDeployer {
    bytecode: include_bytes!("../../../target/wasm64-unknown-unknown/release/counter_deployer_template.wasm"),
};

impl<'a> CounterDeployer<'a> {
    pub fn simple_deploy(
        &self,
        init_value: i32,
        owner: Vec<u8>,
        deploy_nonce: u64,
    ) -> Result<ContractId, ContractError> {
        if owner.len() == 32 {
            let vowner: [u8; 32] = owner.clone().try_into().unwrap();
            uplink::deploy(self.bytecode, Some(&(init_value, false, 0u32, 0u32, owner.clone())), vowner, deploy_nonce)
        } else {
            panic!("The owner must be 32 bytes in length");
        }
    }

    pub fn simple_deploy_fail(
        &self,
        init_value: i32,
        owner: Vec<u8>,
        deploy_nonce: u64,
    ) -> Result<ContractId, ContractError> {
        if owner.len() == 32 {
            let vowner: [u8; 32] = owner.clone().try_into().unwrap();
            uplink::deploy(self.bytecode, Some(&(init_value, true, 0u32, 0u32, owner.clone())), vowner, deploy_nonce)
        } else {
            panic!("The owner must be 32 bytes in length");
        }
    }

    pub fn multiple_deploy(
        &self,
        first_init_value: i32,
        last_init_value: i32,
        owner: Vec<u8>,
        deploy_nonce: u64,
    ) -> Result<Vec<ContractId>, ContractError> {
        if first_init_value > last_init_value {
            return Ok(Vec::new());
        }
        let mut ids: Vec<ContractId>  = uplink::call::<_, Result<Vec<ContractId>, ContractError>>(
            uplink::self_id(),
            "multiple_deploy",
            &(first_init_value + 1, last_init_value, owner.clone(), deploy_nonce + 1)
        )??;
        let new_id = uplink::call::<_, Result<ContractId, ContractError>>(
            uplink::self_id(),
            "simple_deploy",
            &(first_init_value, owner, deploy_nonce)
        )??;
        ids.push(new_id);
        Ok(ids)
    }

    /// Mutually recursive function with the template counter's init.
    /// 
    /// Works as follows:
    /// - Deploys a contract
    /// - If `additional_deploys` != 0, the contract's init function calls this
    ///   function to deploy another contract
    /// - Process repeats until `additional_deploys` == 0
    /// 
    /// `fail_at` tells at what point and additional deploy triggered from the
    /// contract's init function should fail.
    /// `fail` tells whether or not the contract being deployed should panic in its
    /// init function.
    pub fn recursive_deploy_through_init(
        &self,
        init_value: i32,
        fail: bool,
        fail_at: u32,
        additional_deploys: u32,
        deploy_nonce: u64,
        owner: Vec<u8>,
    ) -> Result<ContractId, ContractError> {
        if owner.len() == 32 {
            let vowner: [u8; 32] = owner.clone().try_into().unwrap();
            uplink::deploy(
                self.bytecode,
                Some(&(init_value, fail, fail_at, additional_deploys, owner)),
                vowner,
                deploy_nonce,
            )
        } else {
            panic!("The owner must be 32 bytes in length");
        }
    }
}

/// Expose `CounterDeployer::simple_deploy` to the host
#[no_mangle]
unsafe fn simple_deploy(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |(init_value, owner, nonce)| STATE.simple_deploy(init_value, owner, nonce))
}

/// Expose `CounterDeployer::multiple_deploy` to the host
#[no_mangle]
unsafe fn multiple_deploy(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |(first_init_value, last_init_value, owner, nonce)| STATE.multiple_deploy(first_init_value, last_init_value, owner, nonce))
}

/// Expose `CounterDeployer::simple_deploy_fail` to the host
#[no_mangle]
unsafe fn simple_deploy_fail(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |(init_value, owner, deploy_nonce)| {
        STATE.simple_deploy_fail(init_value, owner, deploy_nonce)
    })
}

/// Expose `CounterDeployer::recursive_deploy_through_init` to the host
#[no_mangle]
unsafe fn recursive_deploy_through_init(arg_len: u32) -> u32 {
    uplink::wrap_call(arg_len, |args| {
        let (init_value,
            fail,
            fail_at,
            additional_deploys,
            deploy_nonce,
            owner
        ) = args;
        STATE.recursive_deploy_through_init(init_value, fail, fail_at, additional_deploys, deploy_nonce, owner)
    })
}
