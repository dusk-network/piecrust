// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use hatchery::{module_bytecode, Error, Receipt, World};

fn hash(buf: &mut [u8], len: u32) -> u32 {
    assert_eq!(len, 4, "the length should come from the module as 4");

    let mut num_bytes = [0; 4];
    num_bytes.copy_from_slice(&buf[..4]);
    let num = i32::from_le_bytes(num_bytes);

    let hash = hash_num(num);
    buf[..32].copy_from_slice(&hash);

    32
}

fn hash_num(num: i32) -> [u8; 32] {
    *blake3::hash(&num.to_le_bytes()).as_bytes()
}

#[test]
pub fn host_hash() -> Result<(), Error> {
    let mut world = World::ephemeral()?;

    let id = world.deploy(module_bytecode!("host"))?;

    world.register_native_query("hash", hash);

    let h: Receipt<[u8; 32]> =
        world.query(id, "hash", 42).expect("query should succeed");
    assert_eq!(hash_num(42), *h);

    Ok(())
}
