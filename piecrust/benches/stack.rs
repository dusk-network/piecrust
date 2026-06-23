// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use piecrust::{ContractData, SessionData, VM, contract_bytecode};

const SAMPLE_SIZE: usize = 10240;
const OWNER: [u8; 32] = [0; 32];
const LIMIT: u64 = 1_000_000;

fn config() -> Criterion {
    Criterion::default().sample_size(SAMPLE_SIZE)
}

fn push(c: &mut Criterion) {
    let vm = VM::ephemeral().expect("Ephemeral VM should succeed");
    let mut session = vm
        .session(SessionData::builder())
        .expect("Session should succeed");

    let (id, _) = session
        .deploy::<_, (), _>(
            contract_bytecode!("stack"),
            ContractData::builder().owner(OWNER),
            LIMIT,
        )
        .expect("Deployment should succeed");

    c.bench_function("push", |b| {
        b.iter(|| {
            let value = black_box(42);
            session
                .call::<_, ()>(id, "push", &value, LIMIT)
                .expect("Pushing should succeed");
        });
    });
}

fn pop(c: &mut Criterion) {
    let vm = VM::ephemeral().expect("Ephemeral VM should succeed");
    let mut session = vm
        .session(SessionData::builder())
        .expect("Session should succeed");

    let (id, _) = session
        .deploy::<_, (), _>(
            contract_bytecode!("stack"),
            ContractData::builder().owner(OWNER),
            LIMIT,
        )
        .expect("Deployment should succeed");

    for _ in 0..SAMPLE_SIZE {
        let value = black_box(42);
        session
            .call::<_, ()>(id, "push", &value, LIMIT)
            .expect("Pushing should succeed");
    }

    c.bench_function("pop", |b| {
        b.iter(|| {
            session
                .call::<_, Option<i32>>(id, "pop", &(), LIMIT)
                .expect("Popping should succeed");
        });
    });
}

criterion_group!(
    name = benches;
    config = config();
    targets = push, pop
);
criterion_main!(benches);
