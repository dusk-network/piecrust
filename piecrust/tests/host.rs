// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use dusk_plonk::prelude::*;
use once_cell::sync::Lazy;
use piecrust::{
    contract_bytecode, ContractData, Error, HostQuery, SessionData, VM,
};
use rand::rngs::OsRng;
use rkyv::Deserialize;
use std::any::Any;

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;

fn get_prover_verifier() -> &'static (Prover, Verifier) {
    static PROVER_VERIFIER: Lazy<(Prover, Verifier)> = Lazy::new(|| {
        let mut rng = OsRng;

        let pp = PublicParameters::setup(1 << 4, &mut rng)
            .expect("Generating public parameters should succeed");
        let label = b"dusk-network";

        let (prover, verifier) = Compiler::compile::<TestCircuit>(&pp, label)
            .expect("Compiling circuit should succeed");

        (prover, verifier)
    });

    &PROVER_VERIFIER
}

fn hash(buf: &mut [u8], len: u32) -> u32 {
    let a = unsafe { rkyv::archived_root::<Vec<u8>>(&buf[..len as usize]) };
    let v: Vec<u8> = a.deserialize(&mut rkyv::Infallible).unwrap();

    let hash = blake3::hash(&v);
    buf[..32].copy_from_slice(&hash.as_bytes()[..]);

    32
}

fn verify_proof(buf: &mut [u8], len: u32) -> u32 {
    let a = unsafe {
        rkyv::archived_root::<(Proof, Vec<BlsScalar>)>(&buf[..len as usize])
    };

    let (proof, public_inputs): (Proof, Vec<BlsScalar>) =
        a.deserialize(&mut rkyv::Infallible).unwrap();

    let (_, verifier) = get_prover_verifier();

    let valid = verifier.verify(&proof, &public_inputs).is_ok();
    let valid_bytes = rkyv::to_bytes::<_, 8>(&valid).unwrap();

    buf[..valid_bytes.len()].copy_from_slice(&valid_bytes);

    valid_bytes.len() as u32
}

struct VeryExpensiveQuery;

impl HostQuery for VeryExpensiveQuery {
    fn deserialize_and_price(
        &self,
        _arg_buf: &[u8],
        _arg: &mut Box<dyn Any>,
    ) -> u64 {
        u64::MAX
    }

    fn execute(&self, _arg: &Box<dyn Any>, _arg_buf: &mut [u8]) -> u32 {
        unreachable!("Query will never be executed since its price is too high")
    }
}

fn new_ephemeral_vm() -> Result<VM, Error> {
    let mut vm = VM::ephemeral()?;
    vm.register_host_query("hash", hash);
    vm.register_host_query("verify_proof", verify_proof);
    vm.register_host_query("very_expensive", VeryExpensiveQuery);
    Ok(vm)
}

#[test]
pub fn host_hash() -> Result<(), Error> {
    let vm = new_ephemeral_vm()?;

    let mut session = vm.session(None, SessionData::builder())?;

    let id =
        session.deploy(None, contract_bytecode!("host"), &(), OWNER, LIMIT)?;

    let v = vec![0u8, 1, 2];
    let h = session
        .call::<_, [u8; 32]>(id, "host_hash", &v, LIMIT)
        .expect("query should succeed")
        .data;
    assert_eq!(blake3::hash(&[0u8, 1, 2]).as_bytes(), &h);

    Ok(())
}

#[test]
pub fn host_very_expensive_oog() -> Result<(), Error> {
    let vm = new_ephemeral_vm()?;

    let mut session = vm.session(None, SessionData::builder())?;

    let id =
        session.deploy(None, contract_bytecode!("host"), &(), OWNER, LIMIT)?;

    let err = session
        .call::<_, String>(id, "host_very_expensive", &(), LIMIT)
        .expect_err("query should fail since it's too expensive");

    assert!(matches!(err, Error::OutOfGas));

    Ok(())
}

/// Proves that we know a number `c` such that `a + b = c`.
#[derive(Default)]
struct TestCircuit {
    a: BlsScalar,
    b: BlsScalar,
    c: BlsScalar,
}

impl Circuit for TestCircuit {
    fn circuit<C>(
        &self,
        composer: &mut C,
    ) -> Result<(), dusk_plonk::error::Error>
    where
        C: Composer,
    {
        let a_w = composer.append_witness(self.a);
        let b_w = composer.append_witness(self.b);

        // q_m · a · b  + q_l · a + q_r · b + q_o · o + q_4 · d + q_c + PI = 0
        //
        // q_m = 0
        // q_l = 1
        // q_r = 1
        // q_o = 0
        // q_4 = 0
        // q_c = 0
        //
        // a + b + PI = 0
        //
        // PI = -c
        //
        // a + b = c
        //
        // PI = -c
        // a + b - c = 0
        let constraint = Constraint::new()
            .left(1)
            .a(a_w)
            .right(1)
            .b(b_w)
            .public(-self.c);
        composer.append_gate(constraint);

        Ok(())
    }
}

#[test]
pub fn host_proof() -> Result<(), Error> {
    let vm = new_ephemeral_vm()?;

    let mut session = vm.session(None, SessionData::builder())?;

    let id =
        session.deploy(None, contract_bytecode!("host"), &(), OWNER, LIMIT)?;

    // 1. Generate proof and public inputs
    let (prover, _) = get_prover_verifier();

    let rng = &mut OsRng;

    let circuit = TestCircuit {
        a: BlsScalar::from(2u64),
        b: BlsScalar::from(2u64),
        c: BlsScalar::from(4u64),
    };

    // 2. Call the contract with the proof and public inputs
    let (proof, public_inputs) =
        prover.prove(rng, &circuit).expect("Proving should succeed");

    let receipt = session.call::<_, String>(
        id,
        "host_verify",
        &(proof, public_inputs),
        LIMIT,
    )?;

    // 3. Assert that this is correct
    assert_eq!(receipt.data, "PROOF IS VALID");

    // 4. Assert that a wrong proof produces the expected result
    let wrong_proof = Proof::default();

    let receipt = session.call::<_, String>(
        id,
        "host_verify",
        &(wrong_proof, vec![BlsScalar::default()]),
        LIMIT,
    )?;

    assert_eq!(receipt.data, "PROOF IS INVALID");

    Ok(())
}
