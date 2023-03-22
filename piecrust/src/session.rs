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
use piecrust_uplink::{ModuleId, SCRATCH_BUF_BYTES};
use rkyv::ser::serializers::{
    BufferScratch, BufferSerializer, CompositeSerializer,
};
use rkyv::ser::Serializer;
use rkyv::{
    check_archived_root, validation::validators::DefaultValidator, Archive,
    Deserialize, Infallible, Serialize,
};
use wasmer_types::WASM_PAGE_SIZE;

use crate::event::Event;
use crate::instance::WrappedInstance;
use crate::module::WrappedModule;
use crate::store::{ModuleSession, Objectcode};
use crate::types::StandardBufSerializer;
use crate::vm::HostQueries;
use crate::Error;
use crate::Error::{InitalizationError, PersistenceError};

use call_stack::{CallStack, StackElement};

const DEFAULT_LIMIT: u64 = 65_536;
const MAX_META_SIZE: usize = 65_536;
pub const CONTRACT_INIT_METHOD: &str = "init";

unsafe impl Send for Session {}
unsafe impl Sync for Session {}

/// A running mutation to a state.
///
/// `Session`s are spawned using a [`VM`] instance, and can be [`queried`] or
/// [`transacted`] with to modify their state. A sequence of these calls may
/// then be [`commit`]ed to, or discarded by simply allowing the session to
/// drop.
///
/// New modules are to be `deploy`ed in the context of a session. Metadata
/// queryable by modules can be set using [`set_meta`].
///
/// [`VM`]: crate::VM
/// [`queried`]: Session::query
/// [`transacted`]: Session::transact
/// [`commit`]: Session::commit
/// [`set_meta`]: Session::set_meta
#[derive(Debug)]
pub struct Session {
    call_stack: CallStack,
    instance_map: BTreeMap<ModuleId, (*mut WrappedInstance, u64)>,
    debug: Vec<String>,
    events: Vec<Event>,
    data: Metadata,

    module_session: ModuleSession,
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
        module_session: ModuleSession,
        host_queries: HostQueries,
    ) -> Self {
        Session {
            call_stack: CallStack::new(),
            instance_map: BTreeMap::new(),
            debug: vec![],
            events: vec![],
            data: Metadata::new(),
            module_session,
            host_queries,
            limit: DEFAULT_LIMIT,
            spent: 0,
            call_history: vec![],
            buffer: vec![0; WASM_PAGE_SIZE],
            call_count: 0,
            icc_count: 0,
            icc_height: 0,
            icc_errors: BTreeMap::new(),
        }
    }

    /// Deploy a module, returning its [`ModuleId`]. The ID is computed using a
    /// `blake3` hash of the `bytecode`.
    ///
    /// If one needs to specify the ID, [`deploy_with_id`] is available.
    ///
    /// [`ModuleId`]: ModuleId
    /// [`deploy_with_id`]: `Session::deploy_with_id`
    pub fn deploy<Arg>(
        &mut self,
        bytecode: &[u8],
        arg: Option<Arg>,
    ) -> Result<ModuleId, Error>
    where
        Arg: for<'b> Serialize<StandardBufSerializer<'b>>,
    {
        let hash = blake3::hash(bytecode);
        let module_id = ModuleId::from_bytes(hash.into());

        self.deploy_with_id(module_id, bytecode, arg)?;

        Ok(module_id)
    }

    /// Deploy a module with the given `id`.
    ///
    /// If one would like to *not* specify the `ModuleId`, [`deploy`] is
    /// available.
    ///
    /// [`deploy`]: `Session::deploy`
    pub fn deploy_with_id<Arg>(
        &mut self,
        id: ModuleId,
        bytecode: &[u8],
        arg: Option<Arg>,
    ) -> Result<(), Error>
    where
        Arg: for<'b> Serialize<StandardBufSerializer<'b>>,
    {
        self.do_deploy_with_id(id, bytecode, arg, None)
    }

    fn do_deploy_with_id<Arg>(
        &mut self,
        id: ModuleId,
        bytecode: &[u8],
        arg: Option<Arg>,
        ser_arg: Option<Vec<u8>>,
    ) -> Result<(), Error>
    where
        Arg: for<'b> Serialize<StandardBufSerializer<'b>>,
    {
        if !self.module_session.module_deployed(id) {
            let wrapped_module =
                WrappedModule::new(bytecode, None::<Objectcode>)?;
            self.module_session
                .deploy_with_id(id, bytecode, wrapped_module.as_bytes())
                .map_err(|err| PersistenceError(Arc::new(err)))?;
        }

        self.create_instance(id)?;

        let instance = self.instance(&id).expect("instance should exist");

        if !matches!((&arg, &ser_arg), (None, None))
            && !instance.is_function_exported(CONTRACT_INIT_METHOD)
        {
            return Err(InitalizationError(
                "deploy initialization failed as init method is not exported"
                    .into(),
            ));
        }

        let s_arg = match (arg, ser_arg) {
            (Some(a), _) => {
                let mut sbuf = [0u8; SCRATCH_BUF_BYTES];
                let scratch = BufferScratch::new(&mut sbuf);
                let ser = BufferSerializer::new(&mut self.buffer[..]);
                let mut ser =
                    CompositeSerializer::new(ser, scratch, Infallible);
                ser.serialize_value(&a).expect("Infallible");
                let pos = ser.pos();
                let v = self.buffer[..pos].to_vec();
                self.initialize(id, v.to_vec())?;
                Some(v)
            }
            (None, Some(v)) => {
                self.initialize(id, v.to_vec())?;
                Some(v)
            }
            _ => None,
        };

        self.call_history.push(From::from(Deploy {
            module_id: id,
            bytecode: bytecode.to_vec(),
            ser_arg: s_arg,
        }));

        Ok(())
    }

    /// Execute a query on the current state of this session.
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
    /// most common ones being running against the point limit and a module
    /// panic. Calling the 'init' method is not allowed except for when
    /// called from the deploy method.
    ///
    /// [`set_point_limit`]: Session::set_point_limit
    /// [`spent`]: Session::spent
    pub fn query<Arg, Ret>(
        &mut self,
        module: ModuleId,
        method_name: &str,
        arg: &Arg,
    ) -> Result<Ret, Error>
    where
        Arg: for<'b> Serialize<StandardBufSerializer<'b>>,
        Ret: Archive,
        Ret::Archived: Deserialize<Ret, Infallible>
            + for<'b> CheckBytes<DefaultValidator<'b>>,
    {
        if method_name == CONTRACT_INIT_METHOD {
            return Err(InitalizationError("init call not allowed".into()));
        }

        let mut sbuf = [0u8; SCRATCH_BUF_BYTES];
        let scratch = BufferScratch::new(&mut sbuf);
        let ser = BufferSerializer::new(&mut self.buffer[..]);
        let mut ser = CompositeSerializer::new(ser, scratch, Infallible);

        ser.serialize_value(arg).expect("Infallible");
        let pos = ser.pos();

        let ret_bytes = self.execute_until_ok(Call {
            ty: CallType::Q,
            module,
            fname: method_name.to_string(),
            fdata: self.buffer[..pos].to_vec(),
            limit: self.limit,
        })?;

        let ta = check_archived_root::<Ret>(&ret_bytes[..])?;
        let ret = ta.deserialize(&mut Infallible).expect("Infallible");

        Ok(ret)
    }

    /// Execute a transaction on the current state of this session.
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
    /// most common ones being running against the point limit and a module
    /// panic. Calling the 'init' method is not allowed except for when
    /// called from the deploy method.
    ///
    /// [`set_point_limit`]: Session::set_point_limit
    /// [`spent`]: Session::spent
    pub fn transact<Arg, Ret>(
        &mut self,
        module: ModuleId,
        method_name: &str,
        arg: &Arg,
    ) -> Result<Ret, Error>
    where
        Arg: for<'b> Serialize<StandardBufSerializer<'b>>,
        Ret: Archive,
        Ret::Archived: Deserialize<Ret, Infallible>
            + for<'b> CheckBytes<DefaultValidator<'b>>,
    {
        if method_name == CONTRACT_INIT_METHOD {
            return Err(InitalizationError("init call not allowed".into()));
        }

        let mut sbuf = [0u8; SCRATCH_BUF_BYTES];
        let scratch = BufferScratch::new(&mut sbuf);
        let ser = BufferSerializer::new(&mut self.buffer[..]);
        let mut ser = CompositeSerializer::new(ser, scratch, Infallible);

        ser.serialize_value(arg).expect("Infallible");
        let pos = ser.pos();

        let ret_bytes = self.execute_until_ok(Call {
            ty: CallType::T,
            module,
            fname: method_name.to_string(),
            fdata: self.buffer[..pos].to_vec(),
            limit: self.limit,
        })?;

        let ta = check_archived_root::<Ret>(&ret_bytes[..])?;
        let ret = ta.deserialize(&mut Infallible).expect("Infallible");

        Ok(ret)
    }

    fn initialize(
        &mut self,
        module: ModuleId,
        arg: Vec<u8>,
    ) -> Result<(), Error> {
        self.execute_until_ok(Call {
            ty: CallType::T,
            module,
            fname: CONTRACT_INIT_METHOD.to_string(),
            fdata: arg,
            limit: self.limit,
        })?;
        Ok(())
    }

    /// Return the state root of the current state of the session.
    ///
    /// The state root is the root of a merkle tree whose leaves are the hashes
    /// of the state of of each module, ordered by their module ID.
    ///
    /// It also doubles as the ID of a commit - the commit root.

    pub fn instance<'a>(
        &self,
        module_id: &ModuleId,
    ) -> Option<&'a mut WrappedInstance> {
        self.instance_map.get(module_id).map(|(instance, _)| {
            // SAFETY: We guarantee that the instance exists since we're in
            // control over if it is dropped in `pop`
            unsafe { &mut **instance }
        })
    }

    fn update_instance_count(&mut self, module_id: ModuleId, inc: bool) {
        match self.instance_map.entry(module_id) {
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
        while self.call_stack.len() > 0 {
            let popped = self.call_stack.pop().unwrap();
            self.remove_instance(&popped.module_id);
        }
        let ids: Vec<ModuleId> = self.instance_map.keys().cloned().collect();
        for module_id in ids.iter() {
            self.remove_instance(module_id);
        }
    }

    pub fn remove_instance(&mut self, module_id: &ModuleId) {
        let mut entry = match self.instance_map.entry(*module_id) {
            Entry::Occupied(e) => e,
            _ => unreachable!("map must have an instance here"),
        };

        let (instance, count) = entry.get_mut();
        *count -= 1;

        if *count == 0 {
            // SAFETY: This is the last instance of the module in the
            // stack, therefore we should recoup the memory and drop
            //
            // Any pointers to it will be left dangling
            unsafe {
                let _ = Box::from_raw(*instance);
                entry.remove();
            };
        }
    }

    pub fn root(&self) -> [u8; 32] {
        self.module_session.root()
    }

    pub(crate) fn push_event(&mut self, event: Event) {
        self.events.push(event);
    }

    fn new_instance(
        &mut self,
        module_id: ModuleId,
    ) -> Result<WrappedInstance, Error> {
        let (bytecode, objectcode, memory) = self
            .module_session
            .module(module_id)
            .map_err(|err| PersistenceError(Arc::new(err)))?
            .expect("Module should exist");

        let module = WrappedModule::new(&bytecode, Some(&objectcode))?;
        let instance = WrappedInstance::new(self, module_id, &module, memory)?;

        Ok(instance)
    }

    pub(crate) fn host_query(
        &self,
        name: &str,
        buf: &mut [u8],
        arg_len: u32,
    ) -> Option<u32> {
        self.host_queries.call(name, buf, arg_len)
    }

    /// Sets the point limit for the next call to [`query`] or [`transact`].
    ///
    /// [`query`]: Session::query
    /// [`transact`]: Session::transact
    pub fn set_point_limit(&mut self, limit: u64) {
        self.limit = limit
    }

    /// Returns the number of points spent by the last call to [`query`] or
    /// [`transact`].
    ///
    /// If neither have been called for the duration of the session, it will
    /// return 0.
    ///
    /// [`query`]: Session::query
    /// [`transact`]: Session::transact
    pub fn spent(&self) -> u64 {
        self.spent
    }

    pub(crate) fn nth_from_top(&self, n: usize) -> Option<StackElement> {
        self.call_stack.nth_from_top(n)
    }

    fn create_instance(&mut self, module_id: ModuleId) -> Result<(), Error> {
        let instance = self.new_instance(module_id)?;
        if self.instance_map.get(&module_id).is_some() {
            panic!("Module already in the stack: {module_id:?}");
        }

        let instance = Box::new(instance);
        let instance = Box::leak(instance) as *mut WrappedInstance;

        self.instance_map.insert(module_id, (instance, 1));
        Ok(())
    }

    pub(crate) fn push_callstack(
        &mut self,
        module_id: ModuleId,
        limit: u64,
    ) -> Result<StackElement, Error> {
        let instance = self.instance(&module_id);

        match instance {
            Some(_) => {
                self.update_instance_count(module_id, true);
                self.call_stack.push(module_id, limit);
            }
            None => {
                self.create_instance(module_id)?;
                self.call_stack.push(module_id, limit);
            }
        }

        Ok(self
            .call_stack
            .nth_from_top(0)
            .expect("We just pushed an element to the stack"))
    }

    pub(crate) fn pop_callstack(&mut self) {
        if let Some(element) = self.call_stack.pop() {
            self.update_instance_count(element.module_id, false);
        }
    }

    /// Commits the given session to disk, consuming the session and returning
    /// its state root.
    pub fn commit(self) -> Result<[u8; 32], Error> {
        self.module_session
            .commit()
            .map_err(|err| PersistenceError(Arc::new(err)))
    }

    pub(crate) fn register_debug<M: Into<String>>(&mut self, msg: M) {
        self.debug.push(msg.into());
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        mem::take(&mut self.events)
    }

    pub fn with_debug<C, R>(&self, c: C) -> R
    where
        C: FnOnce(&[String]) -> R,
    {
        c(&self.debug)
    }

    /// Returns the value of a metadata item previously set using [`set_meta`].
    ///
    /// [`set_meta`]: Session::set_meta
    pub fn meta(&self, name: &str) -> Option<Vec<u8>> {
        self.data.get(name)
    }

    /// Sets a metadata item with the given `name` and `value`. These pieces of
    /// data are then made available to modules for querying.
    pub fn set_meta<S, V>(&mut self, name: S, value: V)
    where
        S: Into<Cow<'static, str>>,
        V: for<'a> Serialize<StandardBufSerializer<'a>>,
    {
        let mut buf = [0u8; MAX_META_SIZE];
        let mut sbuf = [0u8; SCRATCH_BUF_BYTES];

        let ser = BufferSerializer::new(&mut buf[..]);
        let scratch = BufferScratch::new(&mut sbuf);

        let mut serializer =
            StandardBufSerializer::new(ser, scratch, Infallible);
        serializer.serialize_value(&value).expect("Infallible");

        let pos = serializer.pos();

        let data = buf[..pos].to_vec();
        self.data.insert(name, data);
    }

    /// Increment the call execution count.
    ///
    /// If the call errors on the first called module, return said error.
    pub(crate) fn increment_call_count(&mut self) -> Option<Error> {
        self.call_count += 1;
        self.icc_errors
            .get(&self.call_count)
            .and_then(|map| map.get(&0))
            .cloned()
    }

    /// Increment the icc execution count, returning the current count. If there
    /// was, previously, an error in the execution of the ic call with the
    /// current number count - meaning after iteration - it will be returned.
    pub(crate) fn increment_icc_count(&mut self) -> Option<Error> {
        self.icc_count += 1;
        match self.icc_errors.get(&self.call_count) {
            Some(icc_results) => icc_results.get(&self.icc_count).cloned(),
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
        self.icc_count -= 1;
        self.icc_errors
            .get_mut(&self.call_count)
            .expect("Map should always be there")
            .retain(|c, _| c <= &self.icc_count);
    }

    /// Increments the height of an icc.
    pub(crate) fn increment_icc_height(&mut self) {
        self.icc_height += 1;
    }

    /// Decrements the height of an icc.
    pub(crate) fn decrement_icc_height(&mut self) {
        self.icc_height -= 1;
    }

    /// Insert error at the current icc count.
    ///
    /// If there are errors at a larger ICC count than current, they will be
    /// forgotten.
    pub(crate) fn insert_icc_error(&mut self, err: Error) {
        match self.icc_errors.entry(self.call_count) {
            Entry::Vacant(entry) => {
                let mut map = BTreeMap::new();
                map.insert(self.icc_count, err);
                entry.insert(map);
            }
            Entry::Occupied(mut entry) => {
                let map = entry.get_mut();
                map.insert(self.icc_count, err);
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
                if self.icc_height == 0 {
                    self.icc_count = 0;
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
                    if self.icc_height == 0 {
                        self.icc_count = 0;
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
        let mut call_history = Vec::with_capacity(self.call_history.len());
        mem::swap(&mut call_history, &mut self.call_history);

        // Purge all other data that is set by performing transactions.
        self.clear_stack_and_instances();
        self.debug.clear();
        self.events.clear();
        self.module_session.clear_modules();
        self.call_count = 0;

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
                    self.do_deploy_with_id::<()>(deploy.module_id, &deploy.bytecode, None, deploy.ser_arg)
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
        self.icc_count = 0;
        self.icc_height = 0;

        // If we already know of an error on this call, don't execute and just
        // return the error.
        if let Some(err) = self.increment_call_count() {
            // We also need it in the call history here.
            self.call_history.push(call.into());
            return Err(err);
        }

        let res = self.call_inner(&call);
        self.call_history.push(call.into());
        res
    }

    fn call_inner(&mut self, call: &Call) -> Result<Vec<u8>, Error> {
        let stack_element = self.push_callstack(call.module, call.limit)?;
        let instance = self
            .instance(&stack_element.module_id)
            .expect("instance should exist");

        let arg_len = instance.write_bytes_to_arg_buffer(&call.fdata);
        let ret_len = match call.ty {
            CallType::Q => instance.query(&call.fname, arg_len, call.limit),
            CallType::T => instance.transact(&call.fname, arg_len, call.limit),
        }?;
        let ret = instance.read_bytes_from_arg_buffer(ret_len as u32);

        self.spent = call.limit
            - instance
                .get_remaining_points()
                .expect("there should be remaining points");

        self.pop_callstack();

        Ok(ret)
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
    module_id: ModuleId,
    bytecode: Vec<u8>,
    ser_arg: Option<Vec<u8>>,
}

#[derive(Debug)]
enum CallType {
    Q,
    T,
}

#[derive(Debug)]
struct Call {
    ty: CallType,
    module: ModuleId,
    fname: String,
    fdata: Vec<u8>,
    limit: u64,
}

#[derive(Debug)]
pub struct Metadata {
    data: BTreeMap<Cow<'static, str>, Vec<u8>>,
}

impl Metadata {
    fn new() -> Self {
        Self {
            data: BTreeMap::new(),
        }
    }

    fn insert<S>(&mut self, name: S, data: Vec<u8>)
    where
        S: Into<Cow<'static, str>>,
    {
        self.data.insert(name.into(), data);
    }

    fn get(&self, name: &str) -> Option<Vec<u8>> {
        self.data.get(name).cloned()
    }
}
