// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

#[cfg_attr(
    any(target_arch = "wasm32", target_arch = "wasm64"),
    link(wasm_import_module = "env")
)]
unsafe extern "C" {
    #[link_name = "__piecrust_b_call_input_len"]
    pub fn call_input_len() -> u32;
    #[link_name = "__piecrust_b_call_input_copy"]
    pub fn call_input_copy(
        destination: *mut u8,
        input_offset: usize,
        len: u32,
    ) -> i32;
    #[link_name = "__piecrust_b_call_output_set"]
    pub fn call_output_set(source: *const u8, len: u32) -> i32;

    #[link_name = "__piecrust_b_host_result_len"]
    pub fn host_result_len() -> u32;
    #[link_name = "__piecrust_b_host_result_copy"]
    pub fn host_result_copy(
        destination: *mut u8,
        result_offset: usize,
        len: u32,
    ) -> i32;

    #[link_name = "__piecrust_b_host_query"]
    pub fn host_query(
        name: *const u8,
        name_len: u32,
        arg: *const u8,
        arg_len: u32,
    ) -> u32;
    #[link_name = "__piecrust_b_call"]
    pub fn call(
        contract_id: *const u8,
        fn_name: *const u8,
        fn_name_len: u32,
        fn_arg: *const u8,
        fn_arg_len: u32,
        gas_limit: u64,
    ) -> i32;

}
