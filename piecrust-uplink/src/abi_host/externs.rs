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
    pub fn hq_v2(
        name: *const u8,
        name_len: u32,
        arg: *const u8,
        arg_len: u32,
    ) -> u32;
    #[link_name = "__piecrust_b_host_data"]
    pub fn hd_v2(name: *const u8, name_len: u32) -> u32;
    #[link_name = "__piecrust_b_call"]
    pub fn c_v2(
        contract_id: *const u8,
        fn_name: *const u8,
        fn_name_len: u32,
        fn_arg: *const u8,
        fn_arg_len: u32,
        gas_limit: u64,
    ) -> i32;

    #[link_name = "__piecrust_b_emit"]
    pub fn emit_v2(
        topic: *const u8,
        topic_len: u32,
        data: *const u8,
        data_len: u32,
    );
    #[link_name = "__piecrust_b_feed"]
    pub fn feed_v2(data: *const u8, data_len: u32);

    #[link_name = "__piecrust_b_caller"]
    pub fn caller_v2() -> i32;
    #[link_name = "__piecrust_b_callstack"]
    pub fn callstack_v2() -> i32;
    #[link_name = "__piecrust_b_owner"]
    pub fn owner_v2(contract_id: *const u8) -> i32;
    #[link_name = "__piecrust_b_self_id"]
    pub fn self_id_v2();

    pub fn limit() -> u64;
    pub fn spent() -> u64;

    #[cfg(any(target_arch = "wasm32", target_arch = "wasm64"))]
    #[link_name = "__piecrust_b_panic"]
    pub fn panic_v2(message: *const u8, message_len: u32) -> !;

    #[cfg(feature = "debug")]
    #[link_name = "__piecrust_b_debug"]
    pub fn hdebug_v2(message: *const u8, message_len: u32);
}
