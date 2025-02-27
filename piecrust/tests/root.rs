// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust::{
    contract_bytecode, ContractData, Error, PageOpening, SessionData, VM,
};

const OWNER: [u8; 32] = [0u8; 32];
const LIMIT: u64 = 1_000_000;

#[tokio::test(flavor = "multi_thread")]
pub async fn state_root_calculation() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;
    let id_1 = session.deploy(
        contract_bytecode!("counter"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    session.call::<_, ()>(id_1, "increment", &(), LIMIT)?;

    let root_1 = session.root();
    let commit_1 = session.commit()?;

    assert_eq!(
        commit_1, root_1,
        "The commit root is the same as the state root"
    );

    let mut session = vm.session(SessionData::builder().base(commit_1))?;
    let id_2 = session.deploy(
        contract_bytecode!("box"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;
    session.call::<i16, ()>(id_2, "set", &0x11, LIMIT)?;
    session.call::<_, ()>(id_1, "increment", &(), LIMIT)?;

    let root_2 = session.root();
    let commit_2 = session.commit()?;

    assert_eq!(
        commit_2, root_2,
        "The commit root is the same as the state root"
    );
    assert_ne!(
        root_1, root_2,
        "The state root should change since the state changes"
    );

    let session = vm.session(SessionData::builder().base(commit_2))?;
    let root_3 = session.root();

    assert_eq!(root_2, root_3, "The root of a session should be the same if no modifications were made");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
pub async fn inclusion_proofs() -> Result<(), Error> {
    let vm = VM::ephemeral()?;
    let mut session = vm.session(SessionData::builder())?;

    let box_id = session.deploy(
        contract_bytecode!("box"),
        ContractData::builder().owner(OWNER),
        LIMIT,
    )?;

    fn mapper(
        (_, page, opening): (usize, &[u8], PageOpening),
    ) -> (Vec<u8>, PageOpening) {
        (page.to_vec(), opening)
    }

    session.call::<i16, ()>(box_id, "set", &0x11, LIMIT)?;

    let pages = session
        .memory_pages(box_id)
        .expect("There must be memory pages for the contract");

    let (page_1, opening_1) = pages
        .map(mapper)
        .next()
        .expect("There must be at least one page");

    assert!(
        opening_1.verify(&page_1),
        "The page must be valid for the opening"
    );

    session.call::<i16, ()>(box_id, "set", &0x11, LIMIT)?;

    let pages = session
        .memory_pages(box_id)
        .expect("There must be memory pages for the contract");

    let (page_2, opening_2) = pages
        .map(mapper)
        .next()
        .expect("There must be at least one page");

    assert!(
        opening_2.verify(&page_2),
        "The page must be valid for the opening"
    );

    assert!(
        !opening_1.verify(&page_2),
        "The page must be invalid for the opening"
    );
    assert!(
        !opening_2.verify(&page_1),
        "The page must be invalid for the opening"
    );

    Ok(())
}
