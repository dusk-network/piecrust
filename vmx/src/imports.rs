// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use rkyv::{check_archived_value, Archive, Deserialize, Infallible};
use wasmer::{imports, Function, FunctionEnv, FunctionEnvMut};

use uplink::QueryHeader;

use crate::instance::Env;

pub(crate) struct DefaultImports;

impl DefaultImports {
    pub fn default(store: &mut wasmer::Store, env: Env) -> wasmer::Imports {
        let fenv = FunctionEnv::new(store, env);

        #[rustfmt::skip]
        imports! {
            "env" => {
                "caller" => Function::new_typed_with_env(store, &fenv, caller),
		"q" => Function::new_typed_with_env(store, &fenv, q),
		"t" => Function::new_typed_with_env(store, &fenv, t),

		"host_debug" => Function::new_typed_with_env(store, &fenv, host_debug),		
            }
        }
    }
}

fn caller(_env: FunctionEnvMut<Env>) -> u32 {
    0
}

fn q(fenv: FunctionEnvMut<Env>, arg_len: u32) -> u32 {
    let env = fenv.data();

    let instance = env.self_instance();

    instance.with_arg_buffer(|argbuf| {
        let header_archived = check_archived_value::<QueryHeader>(argbuf, 0)
            .expect("all possible bytes valid");
        let header: QueryHeader = header_archived
            .deserialize(&mut Infallible)
            .expect("infallible");

        println!("got header {:?}", header);

        let header_size = core::mem::size_of::<QueryHeader>();
        debug_assert!(
            core::mem::size_of::<QueryHeader>()
                == core::mem::size_of::<<QueryHeader as Archive>::Archived>()
        );

        println!("arg_len {:?}", arg_len);
        println!("header_size {:?}", header_size);
        println!("header.name_len {:?}", header.name_len);

        let query_arg_len = arg_len as usize - header.name_len as usize;

        let arg = &argbuf[header_size..][..query_arg_len];
        let name = core::str::from_utf8(
            &argbuf[(header_size + query_arg_len)..]
                [..header.name_len as usize],
        )
        .expect("TODO error handling");

        let mut callee = env.instance(header.callee);

        callee.copy_argument(arg);
        callee.query(name, arg.len() as u32).expect("invalid query")
    })
}

fn t(_fenv: FunctionEnvMut<Env>, _arg_len: u32) -> u32 {
    todo!()
}

fn host_debug(fenv: FunctionEnvMut<Env>, msg_ofs: i32, msg_len: u32) {
    let env = fenv.data();

    env.self_instance().with_memory(|mem| {
        let slice = &mem[msg_ofs as usize..][..msg_len as usize];
        println!(
            "MODULE DEBUG {:?}",
            std::str::from_utf8(slice).expect("Invalid debug string")
        )
    })
}
