// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use wasmer::{imports, Function, FunctionEnv, FunctionEnvMut};

use uplink::{ModuleId, ARGBUF_LEN};

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

fn q(
    fenv: FunctionEnvMut<Env>,
    mod_id_ofs: i32,
    name_ofs: i32,
    name_len: u32,
    arg_len: u32,
) -> u32 {
    let env = fenv.data();

    let instance = env.self_instance();
    let argbuf_ofs = instance.arg_buffer_offset();

    instance.with_memory_mut(|memory| {
        let (ret_len, mut callee) = {
            let name = core::str::from_utf8(
                &memory[name_ofs as usize..][..name_len as usize],
            )
            .expect("TODO error handling");

            let arg_buf = &memory[argbuf_ofs..][..ARGBUF_LEN];
            let mut mod_id = ModuleId::uninitialized();
            mod_id.as_bytes_mut().copy_from_slice(
                &memory[mod_id_ofs as usize..]
                    [..std::mem::size_of::<ModuleId>()],
            );

            let mut callee = env.instance(mod_id);

            let arg = &arg_buf[..arg_len as usize];

            callee.write_argument(arg);
            let ret_len =
                callee.query(name, arg.len() as u32).expect("invalid query");
            (ret_len, callee)
        };

        // copy back result
        callee.read_argument(&mut memory[argbuf_ofs..][..ret_len as usize]);
        ret_len
    })
}

fn t(
    fenv: FunctionEnvMut<Env>,
    mod_id_ofs: i32,
    name_ofs: i32,
    name_len: u32,
    arg_len: u32,
) -> u32 {
    let env = fenv.data();

    let instance = env.self_instance();
    let argbuf_ofs = instance.arg_buffer_offset();

    instance.with_memory_mut(|memory| {
        let (ret_len, mut callee) = {
            let name = core::str::from_utf8(
                &memory[name_ofs as usize..][..name_len as usize],
            )
            .expect("TODO error handling");

            let arg_buf = &memory[argbuf_ofs..][..ARGBUF_LEN];

            let mut mod_id = ModuleId::uninitialized();
            mod_id.as_bytes_mut().copy_from_slice(
                &memory[mod_id_ofs as usize..]
                    [..std::mem::size_of::<ModuleId>()],
            );

            let mut callee = env.instance(mod_id);

            let arg = &arg_buf[..arg_len as usize];

            callee.write_argument(arg);
            let ret_len = callee
                .transact(name, arg.len() as u32)
                .expect("invalid transaction");
            (ret_len, callee)
        };

        // copy back result
        callee.read_argument(&mut memory[argbuf_ofs..][..ret_len as usize]);
        ret_len
    })
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
