// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

#![feature(proc_macro_quote)]
#![no_std]
extern crate alloc;
extern crate proc_macro;

use alloc::vec::Vec;
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, ImplItem, ItemImpl, Type, TypePath, Visibility};

#[proc_macro_attribute]
pub fn contract(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // Grab an implementation block
    let input_impl = parse_macro_input!(item as ItemImpl);

    // Take the last segment of the impl path.
    let impl_type =
        if let Type::Path(TypePath { path, .. }) = &*input_impl.self_ty {
            path.segments.last().unwrap().ident.clone()
        } else {
            panic!("Expected a type path for the impl block");
        };

    // Will store the generated "wrap_call" functions for each public function
    // on the impl block
    let mut generated_functions = Vec::new();

    for item in input_impl.clone().items {
        if let ImplItem::Fn(method) = item {
            if matches!(method.vis, Visibility::Public(_)) {
                let method_name = method.sig.ident;
                let inputs = method.sig.inputs;

                // Check if the function is an instance method (has `self`)
                let is_instance_method = inputs
                    .iter()
                    .any(|arg| matches!(arg, syn::FnArg::Receiver(_)));

                let generated_function = if is_instance_method {
                    // Determine if the function has additional arguments
                    // besides `self`
                    let has_additional_args = inputs.len() > 1;

                    if has_additional_args {
                        // Prepare a tuple of types for the arguments, skipping
                        // `self`
                        let arg_types: Vec<_> = inputs
                            .iter()
                            .skip(1)
                            .map(|arg| match arg {
                                syn::FnArg::Typed(pat_type) => {
                                    pat_type.ty.clone()
                                }
                                _ => panic!("Expected typed argument"),
                            })
                            .collect();

                        // Generate a tuple pattern to unpack the arguments
                        let arg_pattern: Vec<_> = (0..arg_types.len())
                            .map(|i| format_ident!("arg{}", i))
                            .collect();

                        // For an instance method with 1 or more arguments
                        quote! {
                            #[no_mangle]
                            pub unsafe fn #method_name(arg_len: u32) -> u32 {
                                piecrust_uplink::wrap_call(arg_len, |(#(#arg_pattern),*): (#(#arg_types),*)| {
                                    STATE.#method_name(#(#arg_pattern),*)
                                })
                            }
                        }
                    } else {
                        // For an instance method with no arguments
                        quote! {
                            #[no_mangle]
                            pub unsafe fn #method_name(arg_len: u32) -> u32 {
                                piecrust_uplink::wrap_call(arg_len, |_: ()| STATE.#method_name())
                            }
                        }
                    }
                } else {
                    // Logic for static functions
                    // Prepare a tuple of types for the arguments
                    let arg_types: Vec<_> = inputs
                        .iter()
                        .map(|arg| match arg {
                            syn::FnArg::Typed(pat_type) => pat_type.ty.clone(),
                            _ => panic!("Expected typed argument"),
                        })
                        .collect();

                    // Generate a tuple pattern to unpack the arguments
                    let arg_pattern: Vec<_> = (0..arg_types.len())
                        .map(|i| format_ident!("arg{}", i))
                        .collect();

                    quote! {
                        #[no_mangle]
                        pub unsafe fn #method_name(arg_len: u32) -> u32 {
                            piecrust_uplink::wrap_call(arg_len, |(#(#arg_pattern),*): (#(#arg_types),*)| {
                                #impl_type::#method_name(#(#arg_pattern),*)
                            })
                        }
                    }
                };

                generated_functions.push(generated_function);
            }
        }
    }

    // #input_impl takes the original implementation block and adds the
    // generated "wrap_call" functions for all of the blocks public functions
    // outside the impl block
    let expanded = quote! {
        #input_impl

        #(#generated_functions)*
    };

    TokenStream::from(expanded)
}
