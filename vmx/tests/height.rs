// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use vmx::{module_bytecode, Error, VM};

#[test]
pub fn height() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;

    let id = vm.deploy(module_bytecode!("everest"))?;

    let mut session = vm.session();

    for h in 0..1024 {
        session.set_meta("height", h);
        let height: u64 = session.transact(id, "get_height", ())?;
        assert_eq!(height, h);
    }

    Ok(())
}
