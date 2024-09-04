// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use alloc::vec::Vec;
use core::ptr;

use rkyv::{
    archived_root,
    ser::serializers::{BufferScratch, BufferSerializer, CompositeSerializer},
    ser::Serializer,
    Archive, Deserialize, Infallible, Serialize,
};

use crate::{
    ContractError, ContractId, StandardBufSerializer, CONTRACT_ID_BYTES,
    SCRATCH_BUF_BYTES,
};

pub mod arg_buf {
    use crate::ARGBUF_LEN;
    use core::ptr;
    use core::slice;

    #[no_mangle]
    static mut A: [u64; ARGBUF_LEN / 8] = [0; ARGBUF_LEN / 8];

    pub fn with_arg_buf<F, R>(f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        unsafe {
            let addr = ptr::addr_of_mut!(A);
            let slice = slice::from_raw_parts_mut(addr as _, ARGBUF_LEN);
            f(slice)
        }
    }
}

pub(crate) use arg_buf::with_arg_buf;

mod ext {
    extern "C" {
        pub fn hq(name: *const u8, name_len: u32, arg_len: u32) -> u32;
        pub fn hd(name: *const u8, name_len: u32) -> u32;

        pub fn c(
            contract_id: *const u8,
            fn_name: *const u8,
            fn_name_len: u32,
            fn_arg_len: u32,
            gas_limit: u64,
        ) -> i32;

        pub fn emit(topic: *const u8, topic_len: u32, arg_len: u32);
        pub fn feed(arg_len: u32);

        pub fn caller() -> i32;
        pub fn limit() -> u64;
        pub fn spent() -> u64;
        pub fn owner(contract_id: *const u8) -> i32;
        pub fn self_id();
    }
}

/// Execute some code that the host provides under the given name.
pub fn host_query<A, Ret>(name: &str, arg: A) -> Ret
where
    A: for<'a> Serialize<StandardBufSerializer<'a>>,
    Ret: Archive,
    Ret::Archived: Deserialize<Ret, Infallible>,
{
    let arg_len = with_arg_buf(|buf| {
        let mut sbuf = [0u8; SCRATCH_BUF_BYTES];
        let scratch = BufferScratch::new(&mut sbuf);
        let ser = BufferSerializer::new(buf);
        let mut composite =
            CompositeSerializer::new(ser, scratch, rkyv::Infallible);
        composite.serialize_value(&arg).expect("infallible");
        composite.pos() as u32
    });

    let name_ptr = name.as_bytes().as_ptr();
    let name_len = name.as_bytes().len() as u32;

    let ret_len = unsafe { ext::hq(name_ptr, name_len, arg_len) };

    with_arg_buf(|buf| {
        let slice = &buf[..ret_len as usize];
        let ret = unsafe { archived_root::<Ret>(slice) };
        ret.deserialize(&mut Infallible).expect("Infallible")
    })
}

/// Calls a `contract`'s `fn_name` function with the given argument `fn_arg`.
/// The contract will have `93%` of the remaining gas available to spend.
///
/// To specify the gas allowed to be spent by the called contract, use
/// [`call_with_limit`].
pub fn call<A, Ret>(
    contract: ContractId,
    fn_name: &str,
    fn_arg: &A,
) -> Result<Ret, ContractError>
where
    A: for<'a> Serialize<StandardBufSerializer<'a>>,
    Ret: Archive,
    Ret::Archived: Deserialize<Ret, Infallible>,
{
    call_with_limit(contract, fn_name, fn_arg, 0)
}

/// Calls a `contract`'s `fn_name` function with the given argument `fn_arg`,
/// allowing it to spend the given `gas_limit`.
///
/// A gas limit of `0` is equivalent to using [`call`], and will use the default
/// behavior - i.e. the called contract gets `93%` of the remaining gas.
///
/// If the gas limit given is above or equal the remaining amount, the default
/// behavior will be used instead.
pub fn call_with_limit<A, Ret>(
    contract: ContractId,
    fn_name: &str,
    fn_arg: &A,
    gas_limit: u64,
) -> Result<Ret, ContractError>
where
    A: for<'a> Serialize<StandardBufSerializer<'a>>,
    Ret: Archive,
    Ret::Archived: Deserialize<Ret, Infallible>,
{
    let arg_len = with_arg_buf(|buf| {
        let mut sbuf = [0u8; SCRATCH_BUF_BYTES];
        let scratch = BufferScratch::new(&mut sbuf);
        let ser = BufferSerializer::new(buf);
        let mut composite =
            CompositeSerializer::new(ser, scratch, rkyv::Infallible);
        composite.serialize_value(fn_arg).expect("infallible");
        composite.pos() as u32
    });

    let contract_id_ptr = contract.as_bytes().as_ptr();
    let fn_name = fn_name.as_bytes();

    let ret_len = unsafe {
        ext::c(
            contract_id_ptr,
            fn_name.as_ptr(),
            fn_name.len() as u32,
            arg_len,
            gas_limit,
        )
    };

    with_arg_buf(|buf| {
        if ret_len < 0 {
            Err(ContractError::from_parts(ret_len, buf))
        } else {
            let slice = &buf[..ret_len as usize];
            let ret = unsafe { archived_root::<Ret>(slice) };
            Ok(ret.deserialize(&mut Infallible).expect("Infallible"))
        }
    })
}

/// Calls the function with name `fn_name` of the given `contract` using
/// `fn_arg` as argument.
///
/// To specify the gas allowed to be spent by the called contract, use
/// [`call_raw_with_limit`].
pub fn call_raw(
    contract: ContractId,
    fn_name: &str,
    fn_arg: &[u8],
) -> Result<Vec<u8>, ContractError> {
    call_raw_with_limit(contract, fn_name, fn_arg, 0)
}

/// Calls the function with name `fn_name` of the given `contract` using
/// `fn_arg` as argument, allowing it to spend the given `gas_limit`.
///
/// A gas limit of `0` is equivalent to using [`call_raw`], and will use the
/// default behavior - i.e. the called contract gets `93%` of the remaining gas.
///
/// If the gas limit given is above or equal the remaining amount, the default
/// behavior will be used instead.
pub fn call_raw_with_limit(
    contract: ContractId,
    fn_name: &str,
    fn_arg: &[u8],
    gas_limit: u64,
) -> Result<Vec<u8>, ContractError> {
    with_arg_buf(|buf| {
        buf[..fn_arg.len()].copy_from_slice(fn_arg);
    });

    let fn_name = fn_name.as_bytes();
    let contract_id_ptr = contract.as_bytes().as_ptr();

    let ret_len = unsafe {
        ext::c(
            contract_id_ptr,
            fn_name.as_ptr(),
            fn_name.len() as u32,
            fn_arg.len() as u32,
            gas_limit,
        )
    };

    with_arg_buf(|buf| {
        if ret_len < 0 {
            Err(ContractError::from_parts(ret_len, buf))
        } else {
            Ok(buf[..ret_len as usize].to_vec())
        }
    })
}

/// Returns data made available by the host under the given name. The type `D`
/// must be correctly specified, otherwise undefined behavior will occur.
pub fn meta_data<D>(name: &str) -> Option<D>
where
    D: Archive,
    D::Archived: Deserialize<D, Infallible>,
{
    let name_slice = name.as_bytes();

    let name = name_slice.as_ptr();
    let name_len = name_slice.len() as u32;

    unsafe {
        match ext::hd(name, name_len) as usize {
            0 => None,
            arg_pos => Some(with_arg_buf(|buf| {
                let ret = archived_root::<D>(&buf[..arg_pos]);
                ret.deserialize(&mut Infallible).expect("Infallible")
            })),
        }
    }
}

/// Return the given contract's owner, if the contract exists.
pub fn owner<const N: usize>(contract: ContractId) -> Option<[u8; N]> {
    let contract_id_ptr = contract.as_bytes().as_ptr();

    unsafe {
        match ext::owner(contract_id_ptr) {
            0 => None,
            _ => Some(with_arg_buf(|buf| {
                let ret = archived_root::<[u8; N]>(&buf[..N]);
                ret.deserialize(&mut Infallible).expect("Infallible")
            })),
        }
    }
}

/// Returns the current contract's owner.
pub fn self_owner<const N: usize>() -> [u8; N] {
    unsafe { ext::owner(ptr::null()) };

    with_arg_buf(|buf| {
        let ret = unsafe { archived_root::<[u8; N]>(&buf[..N]) };
        ret.deserialize(&mut Infallible).expect("Infallible")
    })
}

/// Return the current contract's id.
pub fn self_id() -> ContractId {
    unsafe { ext::self_id() };
    with_arg_buf(|buf| {
        let mut bytes = [0; CONTRACT_ID_BYTES];
        bytes.copy_from_slice(&buf[..32]);
        ContractId::from_bytes(bytes)
    })
}

/// Returns the ID of the calling contract, or `None` if this is the first
/// contract to be called.
pub fn caller() -> Option<ContractId> {
    match unsafe { ext::caller() } {
        0 => None,
        _ => with_arg_buf(|buf| {
            let mut bytes = [0; CONTRACT_ID_BYTES];
            bytes.copy_from_slice(&buf[..32]);
            Some(ContractId::from_bytes(bytes))
        }),
    }
}

/// Returns the gas limit with which the contact was called.
pub fn limit() -> u64 {
    unsafe { ext::limit() }
}

/// Returns the amount of gas the contact has spent.
pub fn spent() -> u64 {
    unsafe { ext::spent() }
}

/// Emits an event with the given data, serializing it using [`rkyv`].
pub fn emit<D>(topic: &'static str, data: D)
where
    for<'a> D: Serialize<StandardBufSerializer<'a>>,
{
    with_arg_buf(|buf| {
        let mut sbuf = [0u8; SCRATCH_BUF_BYTES];
        let scratch = BufferScratch::new(&mut sbuf);
        let ser = BufferSerializer::new(buf);
        let mut composite =
            CompositeSerializer::new(ser, scratch, rkyv::Infallible);

        composite.serialize_value(&data).unwrap();
        let arg_len = composite.pos() as u32;

        let topic_ptr = topic.as_ptr();
        let topic_len = topic.len() as u32;

        unsafe { ext::emit(topic_ptr, topic_len, arg_len) }
    });
}

/// Emits an event with the given data.
pub fn emit_raw(topic: &'static str, data: impl AsRef<[u8]>) {
    with_arg_buf(|buf| {
        let data = data.as_ref();

        let arg_len = data.len();
        buf[..arg_len].copy_from_slice(&data);

        let arg_len = arg_len as u32;

        let topic_ptr = topic.as_ptr();
        let topic_len = topic.len() as u32;

        unsafe { ext::emit(topic_ptr, topic_len, arg_len) }
    });
}

/// Feeds the host with data, serializing it using [`rkyv`].
///
/// This is only allowed to be called in the context of a `feed_call`, and
/// will error out otherwise. It is meant for contracts to be able to report
/// large amounts of data to the host, in the span of a single call.
pub fn feed<D>(data: D)
where
    for<'a> D: Serialize<StandardBufSerializer<'a>>,
{
    with_arg_buf(|buf| {
        let mut sbuf = [0u8; SCRATCH_BUF_BYTES];
        let scratch = BufferScratch::new(&mut sbuf);
        let ser = BufferSerializer::new(buf);
        let mut composite =
            CompositeSerializer::new(ser, scratch, rkyv::Infallible);

        composite.serialize_value(&data).unwrap();
        let arg_len = composite.pos() as u32;

        unsafe { ext::feed(arg_len) }
    });
}

/// Feeds the host with data.
///
/// This is only allowed to be called in the context of a `feed_call`, and
/// will error out otherwise. It is meant for contracts to be able to report
/// large amounts of data to the host, in the span of a single call.
pub fn feed_raw(data: impl AsRef<[u8]>) {
    with_arg_buf(|buf| {
        let data = data.as_ref();

        let arg_len = data.len();
        buf[..arg_len].copy_from_slice(&data);

        let arg_len = arg_len as u32;

        unsafe { ext::feed(arg_len) }
    });
}
