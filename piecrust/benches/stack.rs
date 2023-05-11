// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use piecrust::{module_bytecode, VM};

const SAMPLE_SIZE: usize = 10240;

fn config() -> Criterion {
    Criterion::default().sample_size(SAMPLE_SIZE)
}

fn push(c: &mut Criterion) {
    let mut vm = VM::ephemeral().expect("Ephemeral VM should succeed");

    let id = vm
        .deploy(module_bytecode!("stack"))
        .expect("Deployment should succeed");

    let mut session = vm.session();

    c.bench_function("push", |b| {
        b.iter(|| {
            session
                .call::<i32, ()>(id, "push", black_box(42))
                .expect("Pushing should succeed");
        });
    });
}

fn pop(c: &mut Criterion) {
    let mut vm = VM::ephemeral().expect("Ephemeral VM should succeed");

    let id = vm
        .deploy(module_bytecode!("stack"))
        .expect("Deployment should succeed");

    let mut session = vm.session();

    for _ in 0..SAMPLE_SIZE {
        session
            .call(id, "push", black_box(42))
            .expect("Pushing should succeed")
    }

    c.bench_function("pop", |b| {
        b.iter(|| {
            session
                .call::<(), Option<i32>>(id, "pop", ())
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
