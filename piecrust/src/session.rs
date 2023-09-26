// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::borrow::Cow;
use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::mem;
use std::sync::{mpsc, Arc};

use bytecheck::CheckBytes;
use piecrust_uplink::{ContractId, Event, ARGBUF_LEN, SCRATCH_BUF_BYTES};
use rkyv::ser::serializers::{
    BufferScratch, BufferSerializer, CompositeSerializer,
};
use rkyv::ser::Serializer;
use rkyv::{
    check_archived_root, validation::validators::DefaultValidator, Archive,
    Deserialize, Infallible, Serialize,
};
use wasmer_types::WASM_PAGE_SIZE;

use crate::call_tree::{CallTree, CallTreeElem};
use crate::contract::{ContractData, ContractMetadata, WrappedContract};
use crate::error::Error::{self, InitalizationError, PersistenceError};
use crate::instance::WrappedInstance;
use crate::store::{ContractSession, Objectcode};
use crate::types::StandardBufSerializer;
use crate::vm::HostQueries;

const MAX_META_SIZE: usize = ARGBUF_LEN;
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
    call_tree: CallTree,
    instance_map: BTreeMap<ContractId, (*mut WrappedInstance, u64)>,
    debug: Vec<String>,
    data: SessionData,

    contract_session: ContractSession,
    host_queries: HostQueries,
    buffer: Vec<u8>,

    feeder: Option<mpsc::Sender<Vec<u8>>>,
    events: Vec<Event>,
}

impl Session {
    pub(crate) fn new(
        contract_session: ContractSession,
        host_queries: HostQueries,
        data: SessionData,
    ) -> Self {
        Self::from(SessionInner {
            call_tree: CallTree::new(),
            instance_map: BTreeMap::new(),
            debug: vec![],
            data,
            contract_session,
            host_queries,
            buffer: vec![0; WASM_PAGE_SIZE],
            feeder: None,
            events: vec![],
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
    /// Since a deployment may execute some contract initialization code, that
    /// code will be metered and executed with the given `points_limit`.
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
        points_limit: u64,
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

        let mut constructor_arg = None;
        if let Some(arg) = deploy_data.constructor_arg {
            let mut sbuf = [0u8; SCRATCH_BUF_BYTES];
            let scratch = BufferScratch::new(&mut sbuf);
            let ser = BufferSerializer::new(&mut self.inner.buffer[..]);
            let mut ser = CompositeSerializer::new(ser, scratch, Infallible);

            ser.serialize_value(arg)?;
            let pos = ser.pos();

            constructor_arg = Some(self.inner.buffer[0..pos].to_vec());
        }

        let contract_id = deploy_data.contract_id.unwrap();
        self.do_deploy(
            contract_id,
            bytecode,
            constructor_arg,
            deploy_data.owner.to_vec(),
            points_limit,
        )?;

        Ok(contract_id)
    }

    fn do_deploy(
        &mut self,
        contract_id: ContractId,
        bytecode: &[u8],
        arg: Option<Vec<u8>>,
        owner: Vec<u8>,
        points_limit: u64,
    ) -> Result<(), Error> {
        if self.inner.contract_session.contract_deployed(contract_id) {
            return Err(InitalizationError(
                "Deployed error already exists".into(),
            ));
        }

        let wrapped_contract =
            WrappedContract::new(bytecode, None::<Objectcode>)?;
        let contract_metadata = ContractMetadata { contract_id, owner };
        let metadata_bytes = Self::serialize_data(&contract_metadata)?;

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

        let instantiate = || {
            self.create_instance(contract_id)?;
            let instance =
                self.instance(&contract_id).expect("instance should exist");

            let has_init = instance.is_function_exported(INIT_METHOD);
            if has_init && arg.is_none() {
                return Err(InitalizationError(
                    "Contract has constructor but no argument was provided"
                        .into(),
                ));
            }

            if let Some(arg) = arg {
                if !has_init {
                    return Err(InitalizationError(
                        "Argument was provided but contract has no constructor"
                            .into(),
                    ));
                }

                self.call_inner(contract_id, INIT_METHOD, arg, points_limit)?;
            }

            Ok(())
        };

        instantiate().map_err(|err| {
            self.inner.contract_session.remove_contract(&contract_id);
            err
        })
    }

    /// Execute a call on the current state of this session.
    ///
    /// Calls are atomic, meaning that on failure their execution doesn't modify
    /// the state. They are also metered, and will execute with the given
    /// `points_limit`.
    ///
    /// # Errors
    /// The call may error during execution for a wide array of reasons, the
    /// most common ones being running against the point limit and a contract
    /// panic. Calling the 'init' method is not allowed except for when
    /// called from the deploy method.
    pub fn call<A, R>(
        &mut self,
        contract: ContractId,
        fn_name: &str,
        fn_arg: &A,
        points_limit: u64,
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

        ser.serialize_value(fn_arg)?;
        let pos = ser.pos();

        let receipt = self.call_raw(
            contract,
            fn_name,
            self.inner.buffer[..pos].to_vec(),
            points_limit,
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
        points_limit: u64,
    ) -> Result<CallReceipt<Vec<u8>>, Error> {
        if fn_name == INIT_METHOD {
            return Err(InitalizationError("init call not allowed".into()));
        }

        let (data, points_spent, call_tree) =
            self.call_inner(contract, fn_name, fn_arg.into(), points_limit)?;
        let events = mem::take(&mut self.inner.events);

        Ok(CallReceipt {
            points_limit,
            points_spent,
            events,
            call_tree,
            data,
        })
    }

    /// Execute a *feeder* call on the current state of this session.
    ///
    /// Feeder calls are used to have the contract be able to report larger
    /// amounts of data to the host via the channel included in this call.
    ///
    /// These calls are always performed with the maximum amount of points,
    /// since the contracts may spend quite a large amount in an effort to
    /// report data.
    pub fn feeder_call<A, R>(
        &mut self,
        contract: ContractId,
        fn_name: &str,
        fn_arg: &A,
        feeder: mpsc::Sender<Vec<u8>>,
    ) -> Result<CallReceipt<R>, Error>
    where
        A: for<'b> Serialize<StandardBufSerializer<'b>>,
        R: Archive,
        R::Archived: Deserialize<R, Infallible>
            + for<'b> CheckBytes<DefaultValidator<'b>>,
    {
        self.inner.feeder = Some(feeder);
        let r = self.call(contract, fn_name, fn_arg, u64::MAX);
        self.inner.feeder = None;
        r
    }

    /// Execute a raw *feeder* call on the current state of this session.
    ///
    /// See [`feeder_call`] and [`call_raw`] for more information of this type
    /// of call.
    ///
    /// [`feeder_call`]: [`Session::feeder_call`]
    /// [`call_raw`]: [`Session::call_raw`]
    pub fn feeder_call_raw<V: Into<Vec<u8>>>(
        &mut self,
        contract: ContractId,
        fn_name: &str,
        fn_arg: V,
        feeder: mpsc::Sender<Vec<u8>>,
    ) -> Result<CallReceipt<Vec<u8>>, Error> {
        self.inner.feeder = Some(feeder);
        let r = self.call_raw(contract, fn_name, fn_arg, u64::MAX);
        self.inner.feeder = None;
        r
    }

    /// Returns the current length of the memory of the given contract.
    ///
    /// If the contract does not exist, or is otherwise not instantiated in this
    /// session, it will return `None`.
    pub fn memory_len(&self, contract_id: &ContractId) -> Option<usize> {
        self.instance(contract_id)
            .map(|instance| instance.mem_len())
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
        self.inner.call_tree.clear();

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

    pub(crate) fn push_feed(&mut self, data: Vec<u8>) -> Result<(), Error> {
        let feed = self.inner.feeder.as_ref().ok_or(Error::MissingFeed)?;
        feed.send(data).map_err(Error::FeedPulled)
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

    pub(crate) fn nth_from_top(&self, n: usize) -> Option<CallTreeElem> {
        self.inner.call_tree.nth_up(n)
    }

    /// Creates a new instance of the given contract, returning its memory
    /// length.
    fn create_instance(
        &mut self,
        contract: ContractId,
    ) -> Result<usize, Error> {
        let instance = self.new_instance(contract)?;
        if self.inner.instance_map.get(&contract).is_some() {
            panic!("Contract already in the stack: {contract:?}");
        }

        let mem_len = instance.mem_len();

        let instance = Box::new(instance);
        let instance = Box::leak(instance) as *mut WrappedInstance;

        self.inner.instance_map.insert(contract, (instance, 1));
        Ok(mem_len)
    }

    pub(crate) fn push_callstack(
        &mut self,
        contract_id: ContractId,
        limit: u64,
    ) -> Result<CallTreeElem, Error> {
        let instance = self.instance(&contract_id);

        match instance {
            Some(instance) => {
                self.update_instance_count(contract_id, true);
                self.inner.call_tree.push(CallTreeElem {
                    contract_id,
                    limit,
                    spent: 0,
                    mem_len: instance.mem_len(),
                });
            }
            None => {
                let mem_len = self.create_instance(contract_id)?;
                self.inner.call_tree.push(CallTreeElem {
                    contract_id,
                    limit,
                    spent: 0,
                    mem_len,
                });
            }
        }

        Ok(self
            .inner
            .call_tree
            .nth_up(0)
            .expect("We just pushed an element to the stack"))
    }

    pub(crate) fn move_up_call_tree(&mut self, spent: u64) {
        if let Some(element) = self.inner.call_tree.move_up(spent) {
            self.update_instance_count(element.contract_id, false);
        }
    }

    pub(crate) fn move_up_prune_call_tree(&mut self) {
        if let Some(element) = self.inner.call_tree.move_up_prune() {
            self.update_instance_count(element.contract_id, false);
        }
    }

    pub(crate) fn revert_callstack(&mut self) -> Result<(), std::io::Error> {
        for elem in self.inner.call_tree.iter() {
            let instance = self
                .instance(&elem.contract_id)
                .expect("instance should exist");
            instance.revert()?;
            instance.set_len(elem.mem_len);
        }

        Ok(())
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

    pub fn serialize_data<V>(value: &V) -> Result<Vec<u8>, Error>
    where
        V: for<'a> Serialize<StandardBufSerializer<'a>>,
    {
        let mut buf = [0u8; MAX_META_SIZE];
        let mut sbuf = [0u8; SCRATCH_BUF_BYTES];

        let ser = BufferSerializer::new(&mut buf[..]);
        let scratch = BufferScratch::new(&mut sbuf);

        let mut serializer =
            StandardBufSerializer::new(ser, scratch, Infallible);
        serializer.serialize_value(value)?;

        let pos = serializer.pos();

        Ok(buf[..pos].to_vec())
    }

    fn call_inner(
        &mut self,
        contract: ContractId,
        fname: &str,
        fdata: Vec<u8>,
        limit: u64,
    ) -> Result<(Vec<u8>, u64, CallTree), Error> {
        let stack_element = self.push_callstack(contract, limit)?;
        let instance = self
            .instance(&stack_element.contract_id)
            .expect("instance should exist");

        instance
            .snap()
            .map_err(|err| Error::MemorySnapshotFailure {
                reason: None,
                io: Arc::new(err),
            })?;

        let arg_len = instance.write_bytes_to_arg_buffer(&fdata);
        let ret_len = instance
            .call(fname, arg_len, limit)
            .map_err(|err| {
                if let Err(io_err) = self.revert_callstack() {
                    return Error::MemorySnapshotFailure {
                        reason: Some(Arc::new(err)),
                        io: Arc::new(io_err),
                    };
                }
                self.move_up_prune_call_tree();
                err
            })
            .map_err(Error::normalize)?;
        let ret = instance.read_bytes_from_arg_buffer(ret_len as u32);

        let spent = limit
            - instance
                .get_remaining_points()
                .expect("there should be remaining points");

        for elem in self.inner.call_tree.iter() {
            let instance = self
                .instance(&elem.contract_id)
                .expect("instance should exist");
            instance
                .apply()
                .map_err(|err| Error::MemorySnapshotFailure {
                    reason: None,
                    io: Arc::new(err),
                })?;
        }

        let mut call_tree = CallTree::new();
        mem::swap(&mut self.inner.call_tree, &mut call_tree);
        call_tree.update_spent(spent);

        Ok((ret, spent, call_tree))
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
///
/// [`call`]: [`Session::call`]
/// [`call_raw`]: [`Session::call_raw`]
#[derive(Debug)]
pub struct CallReceipt<T> {
    /// The amount of points spent in the execution of the call.
    pub points_spent: u64,
    /// The limit used in during this execution.
    pub points_limit: u64,

    /// The events emitted during the execution of the call.
    pub events: Vec<Event>,
    /// The call tree produced during the execution.
    pub call_tree: CallTree,

    /// The data returned by the called contract.
    pub data: T,
}

impl CallReceipt<Vec<u8>> {
    /// Deserializes a `CallReceipt<Vec<u8>>` into a `CallReceipt<T>` using
    /// `rkyv`.
    fn deserialize<T>(self) -> Result<CallReceipt<T>, Error>
    where
        T: Archive,
        T::Archived: Deserialize<T, Infallible>
            + for<'b> CheckBytes<DefaultValidator<'b>>,
    {
        let ta = check_archived_root::<T>(&self.data[..])?;
        let data = ta.deserialize(&mut Infallible)?;

        Ok(CallReceipt {
            points_spent: self.points_spent,
            points_limit: self.points_limit,
            events: self.events,
            call_tree: self.call_tree,
            data,
        })
    }
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
    pub fn insert<S, V>(mut self, name: S, value: V) -> Result<Self, Error>
    where
        S: Into<Cow<'static, str>>,
        V: for<'a> Serialize<StandardBufSerializer<'a>>,
    {
        let data = Session::serialize_data(&value)?;
        self.data.insert(name.into(), data);
        Ok(self)
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
