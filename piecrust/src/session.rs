// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

pub mod call_stack;

use std::borrow::Cow;
use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::mem;
use std::sync::Arc;

use bytecheck::CheckBytes;
use piecrust_uplink::{ContractId, Event, SCRATCH_BUF_BYTES};
use rkyv::ser::serializers::{
    BufferScratch, BufferSerializer, CompositeSerializer,
};
use rkyv::ser::Serializer;
use rkyv::{
    check_archived_root, validation::validators::DefaultValidator, Archive,
    Deserialize, Infallible, Serialize,
};
use wasmer_types::WASM_PAGE_SIZE;

use crate::contract::{ContractData, ContractMetadata, WrappedContract};
use crate::instance::WrappedInstance;
use crate::store::{ContractSession, Objectcode};
use crate::types::StandardBufSerializer;
use crate::vm::HostQueries;
use crate::Error;
use crate::Error::{InitalizationError, PersistenceError};

use call_stack::{CallStack, StackElement};

const DEFAULT_LIMIT: u64 = 65_536;
const MAX_META_SIZE: usize = 65_536;
pub const INIT_METHOD: &str = "init";

unsafe impl Send for Session {}
unsafe impl Sync for Session {}

/// A running mutation to a state.
///
/// `Session`s are spawned using a [`VM`] instance, and can be used to [`call`]
/// contracts with to modify their state. A sequence of these calls may then be
/// [`commit`]ed to, or discarded by simply allowing the session to drop.
///
/// New contracts are to be `deploy`ed in the context of a session.
///
/// [`VM`]: crate::VM
/// [`call`]: Session::call
/// [`commit`]: Session::commit
#[derive(Debug)]
pub struct Session {
    inner: &'static mut SessionInner,
    original: bool,
}

/// This implementation purposefully boxes and leaks the `SessionInner`.
impl From<SessionInner> for Session {
    fn from(inner: SessionInner) -> Self {
        Self {
            inner: Box::leak(Box::new(inner)),
            original: true,
        }
    }
}

/// A session is created by leaking an using `Box::leak` on a `SessionInner`.
/// Therefore, the memory needs to be recovered.
impl Drop for Session {
    fn drop(&mut self) {
        if self.original {
            // ensure the stack is cleared and all instances are removed and
            // reclaimed on the drop of a session.
            self.clear_stack_and_instances();

            // SAFETY: this is safe since we guarantee that there is no aliasing
            // when a session drops.
            unsafe {
                let _ = Box::from_raw(self.inner);
            }
        }
    }
}

#[derive(Debug)]
struct SessionInner {
    call_stack: CallStack,
    instance_map: BTreeMap<ContractId, (*mut WrappedInstance, u64)>,
    debug: Vec<String>,
    events: Vec<Event>,
    data: SessionData,

    contract_session: ContractSession,
    host_queries: HostQueries,

    limit: u64,
    spent: u64,

    call_history: Vec<CallOrDeploy>,
    buffer: Vec<u8>,

    call_count: usize,
    icc_count: usize, // inter-contract call - 0 is the main call
    icc_height: usize, // height of an inter-contract call
    // Keeps errors/successes that were found during the execution of a
    // particular inter-contract call in the context of a call.
    icc_errors: BTreeMap<usize, BTreeMap<usize, Error>>,
}

impl Session {
    pub(crate) fn new(
        contract_session: ContractSession,
        host_queries: HostQueries,
        data: SessionData,
    ) -> Self {
        Self::from(SessionInner {
            call_stack: CallStack::new(),
            instance_map: BTreeMap::new(),
            debug: vec![],
            events: vec![],
            data,
            contract_session,
            host_queries,
            limit: DEFAULT_LIMIT,
            spent: 0,
            call_history: vec![],
            buffer: vec![0; WASM_PAGE_SIZE],
            call_count: 0,
            icc_count: 0,
            icc_height: 0,
            icc_errors: BTreeMap::new(),
        })
    }

    /// Clone the given session. We explicitly **do not** implement the
    /// [`Clone`] trait here, since we don't want allow the user to clone a
    /// session.
    ///
    /// This is done to allow us to guarantee there is no aliasing of the
    /// reference to `&'static SessionInner`.
    pub(crate) fn clone(&self) -> Self {
        let inner = self.inner as *const SessionInner;
        let inner = inner as *mut SessionInner;
        // SAFETY: we explicitly allow aliasing of the session for internal
        // use.
        Self {
            inner: unsafe { &mut *inner },
            original: false,
        }
    }

    /// Deploy a contract, returning its [`ContractId`]. The ID is computed
    /// using a `blake3` hash of the `bytecode`.
    ///
    /// # Errors
    /// It is possible that a collision between contract IDs occurs, even for
    /// different contract IDs. This is due to the fact that all contracts have
    /// to fit into a sparse merkle tree with `2^32` positions, and as such
    /// a 256-bit number has to be mapped into a 32-bit number.
    ///
    /// If such a collision occurs, [`PersistenceError`] will be returned.
    ///
    /// [`ContractId`]: ContractId
    /// [`PersistenceError`]: PersistenceError
    pub fn deploy<'a, A, D, const N: usize>(
        &mut self,
        bytecode: &[u8],
        deploy_data: D,
    ) -> Result<ContractId, Error>
    where
        A: 'a + for<'b> Serialize<StandardBufSerializer<'b>>,
        D: Into<ContractData<'a, A, N>>,
    {
        let mut deploy_data = deploy_data.into();

        match deploy_data.contract_id {
            Some(_) => (),
            _ => {
                let hash = blake3::hash(bytecode);
                deploy_data.contract_id =
                    Some(ContractId::from_bytes(hash.into()));
            }
        };

        let constructor_arg = deploy_data.constructor_arg.map(|arg| {
            let mut sbuf = [0u8; SCRATCH_BUF_BYTES];
            let scratch = BufferScratch::new(&mut sbuf);
            let ser = BufferSerializer::new(&mut self.inner.buffer[..]);
            let mut ser = CompositeSerializer::new(ser, scratch, Infallible);

            ser.serialize_value(arg).expect("Infallible");
            let pos = ser.pos();

            self.inner.buffer[0..pos].to_vec()
        });

        let contract_id = deploy_data.contract_id.unwrap();
        self.do_deploy(
            contract_id,
            bytecode,
            constructor_arg,
            deploy_data.owner.to_vec(),
        )?;

        Ok(contract_id)
    }

    fn do_deploy(
        &mut self,
        contract_id: ContractId,
        bytecode: &[u8],
        arg: Option<Vec<u8>>,
        owner: Vec<u8>,
    ) -> Result<(), Error> {
        if self.inner.contract_session.contract_deployed(contract_id) {
            return Err(InitalizationError(
                "Deployed error already exists".into(),
            ));
        }

        let wrapped_contract =
            WrappedContract::new(bytecode, None::<Objectcode>)?;
        let contract_metadata = ContractMetadata {
            contract_id,
            owner: owner.clone(),
        };
        let metadata_bytes = Self::serialize_data(&contract_metadata);

        self.inner
            .contract_session
            .deploy(
                contract_id,
                bytecode,
                wrapped_contract.as_bytes(),
                contract_metadata,
                metadata_bytes.as_slice(),
            )
            .map_err(|err| PersistenceError(Arc::new(err)))?;

        self.create_instance(contract_id)?;
        let instance =
            self.instance(&contract_id).expect("instance should exist");

        let has_init = instance.is_function_exported(INIT_METHOD);
        if has_init && arg.is_none() {
            return Err(InitalizationError(
                "Contract has constructor but no argument was provided".into(),
            ));
        }

        if let Some(arg) = &arg {
            if !has_init {
                return Err(InitalizationError(
                    "Argument was provided but contract has no constructor"
                        .into(),
                ));
            }

            self.initialize(contract_id, arg.clone())?;
        }

        self.inner.call_history.push(From::from(Deploy {
            contract_id,
            bytecode: bytecode.to_vec(),
            fdata: arg,
            owner,
        }));

        Ok(())
    }

    /// Execute a call on the current state of this session.
    ///
    /// Calls are atomic, meaning that on failure their execution doesn't modify
    /// the state. They are also metered, and will execute with the point limit
    /// defined in [`set_point_limit`].
    ///
    /// To know how many points a call spent after execution use the [`spent`]
    /// function.
    ///
    /// # Errors
    /// The call may error during execution for a wide array of reasons, the
    /// most common ones being running against the point limit and a contract
    /// panic. Calling the 'init' method is not allowed except for when
    /// called from the deploy method.
    ///
    /// [`set_point_limit`]: Session::set_point_limit
    /// [`spent`]: Session::spent
    pub fn call<A, R>(
        &mut self,
        contract: ContractId,
        fn_name: &str,
        fn_arg: &A,
    ) -> Result<CallReceipt<R>, Error>
    where
        A: for<'b> Serialize<StandardBufSerializer<'b>>,
        R: Archive,
        R::Archived: Deserialize<R, Infallible>
            + for<'b> CheckBytes<DefaultValidator<'b>>,
    {
        if fn_name == INIT_METHOD {
            return Err(InitalizationError("init call not allowed".into()));
        }

        let mut sbuf = [0u8; SCRATCH_BUF_BYTES];
        let scratch = BufferScratch::new(&mut sbuf);
        let ser = BufferSerializer::new(&mut self.inner.buffer[..]);
        let mut ser = CompositeSerializer::new(ser, scratch, Infallible);

        ser.serialize_value(fn_arg).expect("Infallible");
        let pos = ser.pos();

        let receipt = self.call_raw(
            contract,
            fn_name,
            self.inner.buffer[..pos].to_vec(),
        )?;

        receipt.deserialize()
    }

    /// Execute a raw call on the current state of this session.
    ///
    /// Raw calls do not specify the type of the argument or of the return. The
    /// caller is responsible for serializing the argument as the target
    /// `contract` expects.
    ///
    /// For more information about calls see [`call`].
    ///
    /// [`call`]: Session::call
    pub fn call_raw<V: Into<Vec<u8>>>(
        &mut self,
        contract: ContractId,
        fn_name: &str,
        fn_arg: V,
    ) -> Result<CallReceipt<Vec<u8>>, Error> {
        if fn_name == INIT_METHOD {
            return Err(InitalizationError("init call not allowed".into()));
        }

        let data = self.execute_until_ok(Call {
            contract,
            fname: fn_name.to_string(),
            fdata: fn_arg.into(),
            limit: self.inner.limit,
        })?;
        let events = mem::take(&mut self.inner.events);

        Ok(CallReceipt {
            points_limit: self.inner.limit,
            points_spent: self.inner.spent,
            events,
            data,
        })
    }

    pub fn initialize(
        &mut self,
        contract: ContractId,
        arg: Vec<u8>,
    ) -> Result<(), Error> {
        self.execute_until_ok(Call {
            contract,
            fname: INIT_METHOD.to_string(),
            fdata: arg,
            limit: self.inner.limit,
        })?;
        Ok(())
    }

    pub(crate) fn instance<'a>(
        &self,
        contract_id: &ContractId,
    ) -> Option<&'a mut WrappedInstance> {
        self.inner
            .instance_map
            .get(contract_id)
            .map(|(instance, _)| {
                // SAFETY: We guarantee that the instance exists since we're in
                // control over if it is dropped in `pop`
                unsafe { &mut **instance }
            })
    }

    fn update_instance_count(&mut self, contract_id: ContractId, inc: bool) {
        match self.inner.instance_map.entry(contract_id) {
            Entry::Occupied(mut entry) => {
                let (_, count) = entry.get_mut();
                if inc {
                    *count += 1
                } else {
                    *count -= 1
                };
            }
            _ => unreachable!("map must have an instance here"),
        };
    }

    fn clear_stack_and_instances(&mut self) {
        self.inner.call_stack.clear();

        while !self.inner.instance_map.is_empty() {
            let (_, (instance, _)) =
                self.inner.instance_map.pop_first().unwrap();
            unsafe {
                let _ = Box::from_raw(instance);
            };
        }
    }

    /// Return the state root of the current state of the session.
    ///
    /// The state root is the root of a merkle tree whose leaves are the hashes
    /// of the state of of each contract, ordered by their contract ID.
    ///
    /// It also doubles as the ID of a commit - the commit root.
    pub fn root(&self) -> [u8; 32] {
        self.inner.contract_session.root().into()
    }

    pub(crate) fn push_event(&mut self, event: Event) {
        self.inner.events.push(event);
    }

    fn new_instance(
        &mut self,
        contract_id: ContractId,
    ) -> Result<WrappedInstance, Error> {
        let store_data = self
            .inner
            .contract_session
            .contract(contract_id)
            .map_err(|err| PersistenceError(Arc::new(err)))?
            .ok_or(Error::ContractDoesNotExist(contract_id))?;

        let contract = WrappedContract::new(
            store_data.bytecode,
            Some(store_data.objectcode),
        )?;
        let instance = WrappedInstance::new(
            self.clone(),
            contract_id,
            &contract,
            store_data.memory,
        )?;

        Ok(instance)
    }

    pub(crate) fn host_query(
        &self,
        name: &str,
        buf: &mut [u8],
        arg_len: u32,
    ) -> Option<u32> {
        self.inner.host_queries.call(name, buf, arg_len)
    }

    /// Sets the point limit for the next [`call`].
    ///
    /// [`call`]: Session::call
    pub fn set_point_limit(&mut self, limit: u64) {
        self.inner.limit = limit
    }

    /// Returns the number of points spent by the last [`call`].
    ///
    /// If neither have been called for the duration of the session, it will
    /// return 0.
    ///
    /// [`call`]: Session::call
    pub fn spent(&self) -> u64 {
        self.inner.spent
    }

    pub(crate) fn nth_from_top(&self, n: usize) -> Option<StackElement> {
        self.inner.call_stack.nth_from_top(n)
    }

    fn create_instance(
        &mut self,
        contract_id: ContractId,
    ) -> Result<(), Error> {
        let instance = self.new_instance(contract_id)?;
        if self.inner.instance_map.get(&contract_id).is_some() {
            panic!("Contract already in the stack: {contract_id:?}");
        }

        let instance = Box::new(instance);
        let instance = Box::leak(instance) as *mut WrappedInstance;

        self.inner.instance_map.insert(contract_id, (instance, 1));
        Ok(())
    }

    pub(crate) fn push_callstack(
        &mut self,
        contract_id: ContractId,
        limit: u64,
    ) -> Result<StackElement, Error> {
        let instance = self.instance(&contract_id);

        match instance {
            Some(_) => {
                self.update_instance_count(contract_id, true);
                self.inner.call_stack.push(contract_id, limit);
            }
            None => {
                self.create_instance(contract_id)?;
                self.inner.call_stack.push(contract_id, limit);
            }
        }

        Ok(self
            .inner
            .call_stack
            .nth_from_top(0)
            .expect("We just pushed an element to the stack"))
    }

    pub(crate) fn pop_callstack(&mut self) {
        if let Some(element) = self.inner.call_stack.pop() {
            self.update_instance_count(element.contract_id, false);
        }
    }

    /// Commits the given session to disk, consuming the session and returning
    /// its state root.
    pub fn commit(self) -> Result<[u8; 32], Error> {
        self.inner
            .contract_session
            .commit()
            .map(Into::into)
            .map_err(|err| PersistenceError(Arc::new(err)))
    }

    #[cfg(feature = "debug")]
    pub(crate) fn register_debug<M: Into<String>>(&mut self, msg: M) {
        self.inner.debug.push(msg.into());
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        mem::take(&mut self.inner.events)
    }

    pub fn with_debug<C, R>(&self, c: C) -> R
    where
        C: FnOnce(&[String]) -> R,
    {
        c(&self.inner.debug)
    }

    /// Returns the value of a metadata item.
    pub fn meta(&self, name: &str) -> Option<Vec<u8>> {
        self.inner.data.get(name)
    }

    pub fn serialize_data<V>(value: &V) -> Vec<u8>
    where
        V: for<'a> Serialize<StandardBufSerializer<'a>>,
    {
        let mut buf = [0u8; MAX_META_SIZE];
        let mut sbuf = [0u8; SCRATCH_BUF_BYTES];

        let ser = BufferSerializer::new(&mut buf[..]);
        let scratch = BufferScratch::new(&mut sbuf);

        let mut serializer =
            StandardBufSerializer::new(ser, scratch, Infallible);
        serializer.serialize_value(value).expect("Infallible");

        let pos = serializer.pos();

        buf[..pos].to_vec()
    }

    /// Increment the call execution count.
    ///
    /// If the call errors on the first called contract, return said error.
    pub(crate) fn increment_call_count(&mut self) -> Option<Error> {
        self.inner.call_count += 1;
        self.inner
            .icc_errors
            .get(&self.inner.call_count)
            .and_then(|map| map.get(&0))
            .cloned()
    }

    /// Increment the icc execution count, returning the current count. If there
    /// was, previously, an error in the execution of the ic call with the
    /// current number count - meaning after iteration - it will be returned.
    pub(crate) fn increment_icc_count(&mut self) -> Option<Error> {
        self.inner.icc_count += 1;
        match self.inner.icc_errors.get(&self.inner.call_count) {
            Some(icc_results) => {
                icc_results.get(&self.inner.icc_count).cloned()
            }
            None => None,
        }
    }

    /// When this is decremented, it means we have successfully "rolled back"
    /// one icc. Therefore it should remove all errors after the call, after the
    /// decrement.
    ///
    /// # Panics
    /// When the errors map is not present.
    pub(crate) fn decrement_icc_count(&mut self) {
        self.inner.icc_count -= 1;
        self.inner
            .icc_errors
            .get_mut(&self.inner.call_count)
            .expect("Map should always be there")
            .retain(|c, _| c <= &self.inner.icc_count);
    }

    /// Increments the height of an icc.
    pub(crate) fn increment_icc_height(&mut self) {
        self.inner.icc_height += 1;
    }

    /// Decrements the height of an icc.
    pub(crate) fn decrement_icc_height(&mut self) {
        self.inner.icc_height -= 1;
    }

    /// Insert error at the current icc count.
    ///
    /// If there are errors at a larger ICC count than current, they will be
    /// forgotten.
    pub(crate) fn insert_icc_error(&mut self, err: Error) {
        match self.inner.icc_errors.entry(self.inner.call_count) {
            Entry::Vacant(entry) => {
                let mut map = BTreeMap::new();
                map.insert(self.inner.icc_count, err);
                entry.insert(map);
            }
            Entry::Occupied(mut entry) => {
                let map = entry.get_mut();
                map.insert(self.inner.icc_count, err);
            }
        }
    }

    /// Execute the call and re-execute until the call errors with only itself
    /// in the call stack, or succeeds.
    fn execute_until_ok(&mut self, call: Call) -> Result<Vec<u8>, Error> {
        // If the call succeeds at first run, then we can proceed with adding it
        // to the call history and return.
        match self.call_if_not_error(call) {
            Ok(data) => return Ok(data),
            Err(err) => {
                // If the call does not succeed, we should check if it failed at
                // height zero. If so, we should register the error with ICC
                // count 0 and re-execute, returning the result.
                //
                // This will ensure that the call is never really executed,
                // keeping it atomic.
                if self.inner.icc_height == 0 {
                    self.inner.icc_count = 0;
                    self.insert_icc_error(err);
                    return self.re_execute();
                }

                // If it is not at height zero, just register the error and let
                // it re-execute until ok.
                self.insert_icc_error(err);
            }
        }

        // Loop until executed atomically.
        loop {
            match self.re_execute() {
                Ok(awesome) => return Ok(awesome),
                Err(err) => {
                    if self.inner.icc_height == 0 {
                        self.inner.icc_count = 0;
                        self.insert_icc_error(err);
                        return self.re_execute();
                    }
                    self.insert_icc_error(err);
                }
            }
        }
    }

    /// Purge all produced data and re-execute all transactions and deployments
    /// in order, returning the result of the last executed call.
    fn re_execute(&mut self) -> Result<Vec<u8>, Error> {
        // Take all transaction history since we're going to re-add it back
        // anyway.
        let mut call_history =
            Vec::with_capacity(self.inner.call_history.len());
        mem::swap(&mut call_history, &mut self.inner.call_history);

        // Purge all other data that is set by performing transactions.
        self.clear_stack_and_instances();
        self.inner.debug.clear();
        self.inner.events.clear();
        self.inner.contract_session.clear_contracts();
        self.inner.call_count = 0;

        // TODO Figure out how to handle metadata and point limit.
        //      It is important to preserve their value per call.
        //      Right now it probably won't bite us, since we're using it
        //      "properly", and not setting these pieces of data during the
        //      session, but only at the beginning.

        // This will always be set by the loop, so this one will never be
        // returned.
        let mut res = Ok(vec![]);

        for call in call_history {
            match call {
                CallOrDeploy::Call(call) => {
                    res = self.call_if_not_error(call);
                }
                CallOrDeploy::Deploy(deploy) => {
                    self.do_deploy(deploy.contract_id, &deploy.bytecode, deploy.fdata, deploy.owner)
                        .expect("Only deploys that succeed should be added to the history");
                }
            }
        }

        res
    }

    /// Make the call only if an error is not known. If an error is known return
    /// it instead.
    ///
    /// This will add the call to the call history as well.
    fn call_if_not_error(&mut self, call: Call) -> Result<Vec<u8>, Error> {
        // Set both the count and height of the ICCs to zero
        self.inner.icc_count = 0;
        self.inner.icc_height = 0;

        // If we already know of an error on this call, don't execute and just
        // return the error.
        if let Some(err) = self.increment_call_count() {
            // We also need it in the call history here.
            self.inner.call_history.push(call.into());
            return Err(err);
        }

        let res = self.call_inner(&call);
        self.inner.call_history.push(call.into());
        res
    }

    fn call_inner(&mut self, call: &Call) -> Result<Vec<u8>, Error> {
        let stack_element = self.push_callstack(call.contract, call.limit)?;
        let instance = self
            .instance(&stack_element.contract_id)
            .expect("instance should exist");

        let arg_len = instance.write_bytes_to_arg_buffer(&call.fdata);
        let ret_len = instance.call(&call.fname, arg_len, call.limit)?;
        let ret = instance.read_bytes_from_arg_buffer(ret_len as u32);

        self.inner.spent = call.limit
            - instance
                .get_remaining_points()
                .expect("there should be remaining points");

        self.pop_callstack();

        Ok(ret)
    }

    pub fn contract_metadata(
        &self,
        contract_id: &ContractId,
    ) -> Option<&ContractMetadata> {
        self.inner.contract_session.contract_metadata(contract_id)
    }
}

/// The receipt given for a call execution using one of either [`call`] or
/// [`call_raw`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallReceipt<T> {
    /// The amount of points spent in the execution of the call.
    pub points_spent: u64,
    /// The limit used in during this execution.
    pub points_limit: u64,

    /// The events emitted during the execution of the call.
    pub events: Vec<Event>,
    /// The data returned by the called contract.
    pub data: T,
}

impl CallReceipt<Vec<u8>> {
    /// Deserializes a `CallReceipt<Vec<u8>> into a `CallReceipt<T>` using
    /// `rkyv`.
    fn deserialize<T>(self) -> Result<CallReceipt<T>, Error>
    where
        T: Archive,
        T::Archived: Deserialize<T, Infallible>
            + for<'b> CheckBytes<DefaultValidator<'b>>,
    {
        let ta = check_archived_root::<T>(&self.data[..])?;
        let data = ta.deserialize(&mut Infallible).expect("Infallible");

        Ok(CallReceipt {
            points_spent: self.points_spent,
            points_limit: self.points_limit,
            events: self.events,
            data,
        })
    }
}

#[derive(Debug)]
enum CallOrDeploy {
    Call(Call),
    Deploy(Deploy),
}

impl From<Call> for CallOrDeploy {
    fn from(call: Call) -> Self {
        Self::Call(call)
    }
}

impl From<Deploy> for CallOrDeploy {
    fn from(deploy: Deploy) -> Self {
        Self::Deploy(deploy)
    }
}

#[derive(Debug)]
struct Deploy {
    contract_id: ContractId,
    bytecode: Vec<u8>,
    fdata: Option<Vec<u8>>,
    owner: Vec<u8>,
}

#[derive(Debug)]
struct Call {
    contract: ContractId,
    fname: String,
    fdata: Vec<u8>,
    limit: u64,
}

#[derive(Debug, Default)]
pub struct SessionData {
    data: BTreeMap<Cow<'static, str>, Vec<u8>>,
    pub base: Option<[u8; 32]>,
}

impl SessionData {
    pub fn builder() -> SessionDataBuilder {
        SessionDataBuilder {
            data: BTreeMap::new(),
            base: None,
        }
    }

    fn get(&self, name: &str) -> Option<Vec<u8>> {
        self.data.get(name).cloned()
    }
}

impl From<SessionDataBuilder> for SessionData {
    fn from(builder: SessionDataBuilder) -> Self {
        builder.build()
    }
}

pub struct SessionDataBuilder {
    data: BTreeMap<Cow<'static, str>, Vec<u8>>,
    base: Option<[u8; 32]>,
}

impl SessionDataBuilder {
    pub fn insert<S, V>(mut self, name: S, value: V) -> Self
    where
        S: Into<Cow<'static, str>>,
        V: for<'a> Serialize<StandardBufSerializer<'a>>,
    {
        let data = Session::serialize_data(&value);
        self.data.insert(name.into(), data);
        self
    }

    pub fn base(mut self, base: [u8; 32]) -> Self {
        self.base = Some(base);
        self
    }

    fn build(&self) -> SessionData {
        SessionData {
            data: self.data.clone(),
            base: self.base,
        }
    }
}
