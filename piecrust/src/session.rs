// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use std::borrow::Cow;
use std::collections::btree_set::Iter;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Debug, Formatter};
use std::mem;
use std::ptr::NonNull;
use std::sync::{Arc, mpsc};

use bytecheck::CheckBytes;
use dusk_wasmtime::{
    Engine, LinearMemory, MemoryCreator, MemoryType, Module as WasmtimeModule,
};
#[cfg(feature = "call-hook")]
use piecrust_uplink::ContractError;
use piecrust_uplink::{
    ARGBUF_LEN, CONTRACT_ID_BYTES, ContractId, Event, SCRATCH_BUF_BYTES,
};
use rkyv::ser::Serializer;
use rkyv::ser::serializers::{
    BufferScratch, BufferSerializer, CompositeSerializer,
};
use rkyv::validation::validators::DefaultValidator;
use rkyv::{Archive, Deserialize, Infallible, Serialize, check_archived_root};

use crate::call_tree::{CallTree, CallTreeElem};
use crate::contract::{ContractData, ContractMetadata, WrappedContract};
use crate::error::Error::{self, InitalizationError, PersistenceError};
use crate::imports::{callee_gas_limit, check_arg};
use crate::instance::WrappedInstance;
use crate::store::{ContractSession, PAGE_SIZE, PageOpening};
use crate::types::StandardBufSerializer;
use crate::vm::{HostQueries, HostQuery};

const MAX_META_SIZE: usize = ARGBUF_LEN;
// Host stack limits in our current runtime effectively allow around 64 nested
// inter-contract calls; we cap at 48 to keep safety margin. Observed mainnet
// depth is currently <= 6.
pub(crate) const MAX_CALL_DEPTH: usize = 48;
const _: () = assert!(MAX_CALL_DEPTH <= ARGBUF_LEN / CONTRACT_ID_BYTES);
pub const INIT_METHOD: &str = "init";

/// Identity-only context for a root contract call.
///
/// The synthetic caller is exposed through `abi::caller()` and
/// `abi::callstack()`, and is included in call-hook ancestry. It is not an
/// executable contract frame: gas accounting, state snapshots, reentrancy,
/// and the returned call tree continue to describe only the real call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RootCallContext {
    synthetic_caller: ContractId,
}

impl RootCallContext {
    /// Construct a context whose synthetic ancestor is a deployed contract.
    pub const fn synthetic_contract(synthetic_caller: ContractId) -> Self {
        Self { synthetic_caller }
    }

    /// Return the synthetic caller exposed to contracts.
    pub const fn synthetic_caller(self) -> ContractId {
        self.synthetic_caller
    }
}

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
pub struct Session {
    engine: Engine,
    /// Raw pointer to a `Box::leak`-ed `SessionInner`.
    ///
    /// Multiple `Session` clones share this pointer.  The allocation is
    /// stable for the lifetime of the session — it is never moved or
    /// reallocated, an invariant relied upon by `SessionMemoryCreator`,
    /// which captures the pointer at construction.
    ///
    /// Only the `original` session (the one with `original: true`) frees
    /// this allocation on drop.
    inner: NonNull<SessionInner>,
    original: bool,
}

unsafe impl Send for Session {}

impl Debug for Session {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("inner", &self.inner)
            .field("original", &self.original)
            .finish()
    }
}

/// A session is created by leaking an using `Box::leak` on a `SessionInner`.
/// Therefore, the memory needs to be recovered.
impl Drop for Session {
    fn drop(&mut self) {
        if self.original {
            // ensure the stack is cleared and all instances are removed and
            // reclaimed on the drop of a session.
            self.clear_call_tree_and_instances();

            // SAFETY: this is safe since we guarantee that there is no aliasing
            // when a session drops.
            unsafe {
                let _ = Box::from_raw(self.inner.as_ptr());
            }
        }
    }
}

/// The result of a call-hook that intercepted a call on behalf of the VM.
///
/// When a [`CallHook`] returns `Ok(Some(Interception { .. }))`, the VM skips
/// WASM execution, writes `output` to the arg buffer as the call return, and
/// charges `gas_spent` against the caller's remaining gas.
///
/// The intercepted callee is recorded in the call tree but is never
/// instantiated — the VM does not verify that it exists, so a hook can
/// answer calls on behalf of contracts that are not deployed.
#[cfg(feature = "call-hook")]
pub struct Interception {
    /// The serialized return value to write to the arg buffer.
    pub output: Vec<u8>,
    /// Gas to charge the caller for this intercepted call.
    pub gas_spent: u64,
}

/// Context passed to a [`CallHook`], allowing it to inspect the call stack,
/// make nested contract calls, and emit events through the session's call
/// machinery.
///
/// # Call stack
///
/// [`callstack`](HookContext::callstack) returns the call stack at the time
/// the hook fires, ordered like `abi::callstack()`: index 0 is the immediate
/// caller and the last element is the root contract call.  This allows a
/// hook to check any ancestor in the call chain, not just the immediate
/// caller — e.g. to reject a call when a given contract appears anywhere
/// above the callee.
///
/// # Nested calls
///
/// [`call_as`](HookContext::call_as) executes a contract call with a
/// specified caller identity.  Gas accounting, call stack management,
/// and state rollback on failure are handled by the VM.
///
/// Re-entrancy works naturally: if the called contract makes another
/// inter-contract call, the hook fires again with a fresh `HookContext`.
///
/// # Rollback semantics
///
/// When a [`call_as`](HookContext::call_as) call fails, **only that
/// callee's state** is reverted — earlier successful `call_as` mutations
/// within the same hook invocation are preserved in the session.
///
/// However, the hook is expected to propagate the error by returning
/// `Err(contract_error)`.  This surfaces as the same [`ContractError`] to
/// the outer WASM caller (the contract whose ICC was intercepted).  If the
/// outer caller
/// does not handle the error (e.g. it calls `.unwrap()`), it panics and
/// the normal ICC error path in the VM reverts the **entire call subtree**
/// — including any mutations made by earlier successful `call_as` calls
/// in the hook.
///
/// In short: individual `call_as` failures are scoped, but unhandled
/// errors cascade through the WASM call chain and revert everything,
/// matching the transaction semantics of the contract execution path.
///
/// # Events
///
/// [`emit`](HookContext::emit) pushes events into the session's event
/// stream in execution order, interleaved with events from WASM
/// contracts — not appended at the end.
#[cfg(feature = "call-hook")]
pub struct HookContext {
    session: Session,
    /// Indices of events pushed through [`HookContext::emit`], so that
    /// exactly these — and not events of successful nested calls, whose
    /// state persists — can be marked reverted when the hook rejects or
    /// its interception fails.
    emitted: Vec<usize>,
}

#[cfg(feature = "call-hook")]
impl HookContext {
    /// Return the current call stack, ordered like `abi::callstack()`: index
    /// 0 is the immediate caller, and the last element is the root contract
    /// call.
    ///
    /// The callee whose call triggered the hook is not included, because the
    /// hook runs before the callee is pushed.  Caller identity frames pushed
    /// by [`Session::call_as`] are included, as is the synthetic caller of a
    /// contextual root call.
    pub fn callstack(&self) -> Vec<ContractId> {
        self.session
            .effective_call_ids()
            .into_iter()
            .copied()
            .collect()
    }

    /// Execute a contract call with a specified caller identity.
    ///
    /// This is the hook equivalent of [`Session::call_as_raw`]: it pushes a
    /// lightweight caller frame, executes the callee through WASM, and
    /// returns the raw output bytes together with the gas spent.
    ///
    /// The nested call itself does not fire the hook — only inter-contract
    /// calls made by the nested callee do. A hook that enforces policy
    /// checks must therefore apply them to its own `call_as` calls itself.
    ///
    /// The nested call is paid for by the contract whose call the hook
    /// intercepted, and `gas_limit` follows the inter-contract-call
    /// convention: an explicit positive limit below that contract's
    /// remaining gas is used as-is, while `0` or an over-budget limit
    /// selects the default share of the remaining gas. The held-back
    /// reserve keeps a *failed* nested call — which charges its full
    /// limit, exactly like a failed inter-contract call — from starving
    /// the outer frames into `OutOfGas` while the error propagates.
    ///
    /// On error, the callee's state is reverted and the error is returned.
    /// State changes from earlier successful `call_as` calls in the same
    /// hook invocation remain in the session until the outer call chain
    /// either commits or reverts them — see [rollback
    /// semantics](HookContext#rollback-semantics) on `HookContext`.
    pub fn call_as(
        &mut self,
        caller: ContractId,
        callee: ContractId,
        fn_name: &str,
        fn_args: &[u8],
        gas_limit: u64,
    ) -> Result<(Vec<u8>, u64), Error> {
        self.session
            .call_nested(caller, callee, fn_name, fn_args, gas_limit)
    }

    /// Push an event into the session's event stream.
    ///
    /// Events emitted here are interleaved with events from WASM contracts
    /// in execution order, matching the ordering of `abi::emit`. Unlike
    /// `abi::emit`, no gas is charged — a hook that wants the emission
    /// metered must account for it through [`Interception::gas_spent`].
    ///
    /// If the hook rejects the call or its interception fails, events
    /// emitted here are marked as reverted. If the hook allows the call
    /// (`Ok(None)`), its events stay live even when the callee later
    /// fails — like events a caller emits before a failing inter-contract
    /// call.
    pub fn emit(&mut self, event: Event) {
        self.emitted.push(self.session.event_checkpoint());
        self.session.push_event(event);
    }
}

/// A hook called before each inter-contract call and contextual root call.
///
/// Receives the caller contract ID, the callee contract ID, the function name,
/// the raw argument bytes, and a [`HookContext`] for inspecting the call stack
/// and making nested calls.
/// Returns:
/// - `Ok(None)` to allow the call and proceed with normal WASM execution
/// - `Ok(Some(interception))` to short-circuit: the hook handled this call,
///   [`Interception::output`] is the return value and
///   [`Interception::gas_spent`] is charged against the caller
/// - `Err(contract_error)` to reject the call. The [`ContractError`] variant is
///   surfaced to the calling contract as the call's result, so the hook can
///   reproduce any variant — `Panic(msg)`, `Unknown`, `OutOfGas`, or
///   `DoesNotExist` — that the normal execution path would have produced for
///   the same failure. Unlike a real failed callee, which charges the caller
///   its full gas limit, a rejection itself charges no gas (gas spent by the
///   hook's nested calls remains charged).
///
/// The full call stack at the time of the call is available through
/// [`HookContext::callstack`], ordered with the immediate caller first and
/// the root contract call last.
#[cfg(feature = "call-hook")]
pub type CallHook = Arc<
    dyn Fn(
            &ContractId,
            &ContractId,
            &str,
            &[u8],
            &mut HookContext,
        ) -> Result<Option<Interception>, ContractError>
        + Send
        + Sync,
>;

struct SessionInner {
    current: ContractId,

    call_tree: CallTree,
    root_call_context: Option<RootCallContext>,
    instances: BTreeMap<ContractId, Box<WrappedInstance>>,
    compiled_modules: BTreeMap<ContractId, WasmtimeModule>,
    debug: Vec<String>,
    data: SessionData,

    contract_session: ContractSession,
    host_queries: HostQueries,
    buffer: Vec<u8>,

    feeder: Option<mpsc::Sender<Vec<u8>>>,
    events: Vec<Event>,

    #[cfg(feature = "call-hook")]
    call_hook: Option<CallHook>,
}

impl Debug for SessionInner {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionInner")
            .field("current", &self.current)
            .field("call_tree", &self.call_tree)
            .field("root_call_context", &self.root_call_context)
            .field("instances_len", &self.instances.len())
            .field("compiled_modules_len", &self.compiled_modules.len())
            .field("debug_len", &self.debug.len())
            .field("data", &self.data)
            .field("buffer_len", &self.buffer.len())
            .field("events_len", &self.events.len())
            .finish()
    }
}

struct RootCallContextGuard {
    inner: NonNull<SessionInner>,
}

impl Drop for RootCallContextGuard {
    fn drop(&mut self) {
        // SAFETY: the guard cannot outlive the mutable Session call that
        // created it. It only clears the context installed by that call.
        unsafe {
            self.inner.as_mut().root_call_context = None;
        }
    }
}

struct SessionMemoryCreator {
    inner: NonNull<SessionInner>,
}

unsafe impl Send for SessionMemoryCreator {}

unsafe impl Sync for SessionMemoryCreator {}

unsafe impl MemoryCreator for SessionMemoryCreator {
    /// This new memory is created for the contract currently at the top of the
    /// call tree.
    fn new_memory(
        &self,
        _ty: MemoryType,
        minimum: usize,
        _maximum: Option<usize>,
        _reserved_size_in_bytes: Option<usize>,
        _guard_size_in_bytes: usize,
    ) -> Result<Box<dyn LinearMemory>, String> {
        // SAFETY: wasmtime calls this synchronously during instance creation.
        // No other path concurrently accesses `SessionInner` in this callback.
        let inner = unsafe { &mut *self.inner.as_ptr() };
        let contract = inner.current;

        let contract_data =
            inner.contract_session.contract(contract).map_err(|err| {
                format!("Failed to get contract from session: {err:?}")
            })?;

        let mut memory = contract_data
            .expect("Contract data should exist at this point")
            .memory;

        if memory.is_new() {
            memory.set_current_len(minimum);
        }

        Ok(Box::new(memory))
    }
}

impl Session {
    fn inner(&self) -> &SessionInner {
        unsafe { self.inner.as_ref() }
    }

    fn inner_mut(&mut self) -> &mut SessionInner {
        unsafe { self.inner.as_mut() }
    }

    pub(crate) fn new(
        engine: Engine,
        contract_session: ContractSession,
        host_queries: HostQueries,
        data: SessionData,
    ) -> Self {
        let inner = SessionInner {
            current: ContractId::from_bytes([0; CONTRACT_ID_BYTES]),
            call_tree: CallTree::new(),
            root_call_context: None,
            instances: BTreeMap::new(),
            compiled_modules: BTreeMap::new(),
            debug: vec![],
            data,
            contract_session,
            host_queries,
            buffer: vec![0; PAGE_SIZE],
            feeder: None,
            events: vec![],
            #[cfg(feature = "call-hook")]
            call_hook: None,
        };

        // This implementation purposefully boxes and leaks the `SessionInner`.
        let inner = Box::leak(Box::new(inner));

        let mut session = Self {
            engine: engine.clone(),
            inner: NonNull::from(inner),
            original: true,
        };

        let mut config = engine.config().clone();
        config.with_host_memory(Arc::new(SessionMemoryCreator {
            inner: session.inner,
        }));

        session.engine = Engine::new(&config)
            .expect("Engine configuration is set at compile time");

        session
    }

    /// Clone the given session. We explicitly **do not** implement the
    /// [`Clone`] trait here, since we don't want allow the user to clone a
    /// session.
    ///
    /// This keeps cloning internal and preserves ownership/drop invariants
    /// around the shared `SessionInner` pointer.
    pub(crate) fn clone(&self) -> Self {
        Self {
            engine: self.engine.clone(),
            inner: self.inner,
            original: false,
        }
    }

    /// Return a reference to the engine used in this session.
    pub(crate) fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Deploy a contract, returning its [`ContractId`] and an optional
    /// [`CallReceipt`] for the `init` function execution. The ID is computed
    /// using a `blake3` hash of the `bytecode`. Contracts using the `memory64`
    /// proposal are accepted in just the same way as 32-bit contracts, and
    /// their handling is totally transparent.
    ///
    /// Since a deployment may execute some contract initialization code, that
    /// code will be metered and executed with the given `gas_limit`. If the
    /// contract exports an `init` function, the returned receipt contains
    /// the gas spent, events emitted, and call tree produced by that call.
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
    /// [`CallReceipt`]: CallReceipt
    /// [`PersistenceError`]: PersistenceError
    ///
    /// # Panics
    /// If `deploy_data` does not specify an owner, this will panic.
    pub fn deploy<'a, A, R, D>(
        &mut self,
        bytecode: &[u8],
        deploy_data: D,
        gas_limit: u64,
    ) -> Result<(ContractId, Option<CallReceipt<R>>), Error>
    where
        A: 'a + for<'b> Serialize<StandardBufSerializer<'b>>,
        R: Archive,
        R::Archived: Deserialize<R, Infallible>
            + for<'b> CheckBytes<DefaultValidator<'b>>,
        D: Into<ContractData<'a, A>>,
    {
        let deploy_data = deploy_data.into();

        let mut init_arg = None;
        if let Some(arg) = deploy_data.init_arg {
            let mut sbuf = [0u8; SCRATCH_BUF_BYTES];
            let scratch = BufferScratch::new(&mut sbuf);
            let ser = BufferSerializer::new(&mut self.inner_mut().buffer[..]);
            let mut ser = CompositeSerializer::new(ser, scratch, Infallible);

            ser.serialize_value(arg)?;
            let pos = ser.pos();

            init_arg = Some(self.inner().buffer[0..pos].to_vec());
        }

        let (contract_id, receipt) = self.deploy_raw(
            deploy_data.contract_id,
            bytecode,
            init_arg,
            deploy_data
                .owner
                .expect("Owner must be specified when deploying a contract"),
            gas_limit,
        )?;

        let receipt = receipt.map(|r| r.deserialize()).transpose()?;
        Ok((contract_id, receipt))
    }

    /// Deploy a contract, returning its [`ContractId`] and an optional
    /// [`CallReceipt`] for the `init` function execution. If ID is not
    /// provided, it is computed using a `blake3` hash of the `bytecode`.
    /// Contracts using the `memory64` proposal are accepted in just the same
    /// way as 32-bit contracts, and their handling is totally transparent.
    ///
    /// Since a deployment may execute some contract initialization code, that
    /// code will be metered and executed with the given `gas_limit`. If the
    /// contract exports an `init` function, the returned receipt contains
    /// the gas spent, events emitted, and call tree produced by that call.
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
    /// [`CallReceipt`]: CallReceipt
    /// [`PersistenceError`]: PersistenceError
    #[allow(clippy::type_complexity)]
    pub fn deploy_raw(
        &mut self,
        contract_id: Option<ContractId>,
        bytecode: &[u8],
        init_arg: Option<Vec<u8>>,
        owner: Vec<u8>,
        gas_limit: u64,
    ) -> Result<(ContractId, Option<CallReceipt<Vec<u8>>>), Error> {
        let contract_id = contract_id.unwrap_or({
            let hash = blake3::hash(bytecode);
            ContractId::from_bytes(hash.into())
        });
        let receipt =
            self.do_deploy(contract_id, bytecode, init_arg, owner, gas_limit)?;

        Ok((contract_id, receipt))
    }

    #[allow(clippy::too_many_arguments)]
    fn do_deploy(
        &mut self,
        contract_id: ContractId,
        bytecode: &[u8],
        arg: Option<Vec<u8>>,
        owner: Vec<u8>,
        gas_limit: u64,
    ) -> Result<Option<CallReceipt<Vec<u8>>>, Error> {
        if self
            .inner_mut()
            .contract_session
            .contract_deployed(contract_id)
        {
            return Err(InitalizationError(
                "Deployed error already exists".into(),
            ));
        }

        let wrapped_contract =
            WrappedContract::new(&self.engine, bytecode, None::<&[u8]>)?;
        let contract_metadata = ContractMetadata { contract_id, owner };
        let metadata_bytes = Self::serialize_data(&contract_metadata)?;

        self.inner_mut()
            .contract_session
            .deploy(
                contract_id,
                bytecode,
                wrapped_contract.as_bytes(),
                contract_metadata,
                metadata_bytes.as_slice(),
            )
            .map_err(|err| PersistenceError(Arc::new(err)))?;
        self.inner_mut().compiled_modules.remove(&contract_id);

        let instantiate = || {
            self.create_instance(contract_id)?;
            let has_init = self
                .instance(&contract_id)
                .expect("instance should exist")
                .is_function_exported(INIT_METHOD);

            if has_init {
                // If no argument was provided, we call the init method anyway,
                // but with an empty argument. The alternative is to panic, but
                // that assumes that the caller of `deploy` knows that the
                // contract has an init method in the first place, which might
                // not be the case, such as when ingesting untrusted bytecode.
                let arg = arg.unwrap_or_default();
                let (data, gas_spent, call_tree) = self.call_inner(
                    None,
                    contract_id,
                    INIT_METHOD,
                    arg,
                    gas_limit,
                )?;
                let events = mem::take(&mut self.inner_mut().events);
                return Ok(Some(CallReceipt {
                    gas_limit,
                    gas_spent,
                    events,
                    call_tree,
                    data,
                }));
            }

            Ok(None)
        };

        instantiate().inspect_err(|_| {
            self.inner_mut()
                .contract_session
                .remove_contract(&contract_id);
            self.inner_mut().compiled_modules.remove(&contract_id);
        })
    }

    /// Execute a call on the current state of this session.
    ///
    /// Calls are atomic, meaning that on failure their execution doesn't modify
    /// the state. They are also metered, and will execute with the given
    /// `gas_limit`. This value should never be 0.
    ///
    /// # Errors
    /// The call may error during execution for a wide array of reasons, the
    /// most common ones being running against the gas limit and a contract
    /// panic. Calling the 'init' method is not allowed except for when called
    /// from the deploy method.
    ///
    /// If this returns [`Error::MemorySnapshotFailure`], the session's
    /// in-memory execution state may be inconsistent. The session should be
    /// discarded and not used for further calls, deployments, or commits.
    /// Recovery should be performed by creating a fresh session from the last
    /// committed state.
    pub fn call<A, R>(
        &mut self,
        contract: ContractId,
        fn_name: &str,
        fn_arg: &A,
        gas_limit: u64,
    ) -> Result<CallReceipt<R>, Error>
    where
        A: for<'b> Serialize<StandardBufSerializer<'b>>,
        A::Archived: for<'b> CheckBytes<DefaultValidator<'b>>,
        R: Archive,
        R::Archived: Deserialize<R, Infallible>
            + for<'b> CheckBytes<DefaultValidator<'b>>,
    {
        if fn_name == INIT_METHOD {
            return Err(InitalizationError("init call not allowed".into()));
        }

        let mut sbuf = [0u8; SCRATCH_BUF_BYTES];
        let scratch = BufferScratch::new(&mut sbuf);
        let ser = BufferSerializer::new(&mut self.inner_mut().buffer[..]);
        let mut ser = CompositeSerializer::new(ser, scratch, Infallible);

        ser.serialize_value(fn_arg)?;
        let pos = ser.pos();

        let receipt = self.call_raw(
            contract,
            fn_name,
            self.inner().buffer[..pos].to_vec(),
            gas_limit,
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
    /// The same error and recovery semantics apply.
    ///
    /// [`call`]: Session::call
    pub fn call_raw<V: Into<Vec<u8>>>(
        &mut self,
        contract: ContractId,
        fn_name: &str,
        fn_arg: V,
        gas_limit: u64,
    ) -> Result<CallReceipt<Vec<u8>>, Error> {
        if fn_name == INIT_METHOD {
            return Err(InitalizationError("init call not allowed".into()));
        }

        let (data, gas_spent, call_tree) =
            self.call_inner(None, contract, fn_name, fn_arg.into(), gas_limit)?;
        let events = mem::take(&mut self.inner_mut().events);

        Ok(CallReceipt {
            gas_limit,
            gas_spent,
            events,
            call_tree,
            data,
        })
    }

    /// Execute a call on the current state of this session with a specified
    /// caller identity.
    ///
    /// Behaves like [`call`], but inside the callee `abi::caller()` will
    /// return `caller` and `abi::callstack()` will return `[caller]`,
    /// matching the stack depth of a normal contract-to-contract call.
    ///
    /// The receipt's call tree is rooted at the callee, exactly as for a
    /// plain [`call`] — the caller identity frame is internal and not part
    /// of the iterated tree.
    ///
    /// [`call`]: Session::call
    pub fn call_as<A, R>(
        &mut self,
        caller: ContractId,
        contract: ContractId,
        fn_name: &str,
        fn_arg: &A,
        gas_limit: u64,
    ) -> Result<CallReceipt<R>, Error>
    where
        A: for<'b> Serialize<StandardBufSerializer<'b>>,
        A::Archived: for<'b> CheckBytes<DefaultValidator<'b>>,
        R: Archive,
        R::Archived: Deserialize<R, Infallible>
            + for<'b> CheckBytes<DefaultValidator<'b>>,
    {
        let mut sbuf = [0u8; SCRATCH_BUF_BYTES];
        let scratch = BufferScratch::new(&mut sbuf);
        let ser = BufferSerializer::new(&mut self.inner_mut().buffer[..]);
        let mut ser = CompositeSerializer::new(ser, scratch, Infallible);

        ser.serialize_value(fn_arg)?;
        let pos = ser.pos();

        let receipt = self.call_as_raw(
            caller,
            contract,
            fn_name,
            self.inner().buffer[..pos].to_vec(),
            gas_limit,
        )?;

        receipt.deserialize()
    }

    /// Execute a raw call with a specified caller identity.
    ///
    /// See [`call_as`] and [`call_raw`] for more information.
    ///
    /// [`call_as`]: Session::call_as
    /// [`call_raw`]: Session::call_raw
    pub fn call_as_raw<V: Into<Vec<u8>>>(
        &mut self,
        caller: ContractId,
        contract: ContractId,
        fn_name: &str,
        fn_arg: V,
        gas_limit: u64,
    ) -> Result<CallReceipt<Vec<u8>>, Error> {
        if fn_name == INIT_METHOD {
            return Err(InitalizationError("init call not allowed".into()));
        }

        let (data, gas_spent, call_tree) = self.call_inner(
            Some(caller),
            contract,
            fn_name,
            fn_arg.into(),
            gas_limit,
        )?;
        let events = mem::take(&mut self.inner_mut().events);

        Ok(CallReceipt {
            gas_limit,
            gas_spent,
            events,
            call_tree,
            data,
        })
    }

    /// Execute a raw root call with an identity-only synthetic caller.
    ///
    /// This is intended for host execution paths that preserve the caller
    /// identity of an existing contract entry point without executing that
    /// contract as a frame. The synthetic caller must be a deployed contract.
    /// The context is visible to contract caller/call-stack queries and call
    /// hooks for this call only.
    pub fn call_raw_with_root_context<V: Into<Vec<u8>>>(
        &mut self,
        context: RootCallContext,
        contract: ContractId,
        fn_name: &str,
        fn_arg: V,
        gas_limit: u64,
    ) -> Result<CallReceipt<Vec<u8>>, Error> {
        if fn_name == INIT_METHOD {
            return Err(InitalizationError("init call not allowed".into()));
        }
        if self.inner().call_tree.depth() != 0
            || self.inner().root_call_context.is_some()
        {
            return Err(Error::SessionError(
                "root call context requires an idle session".into(),
            ));
        }
        if self.memory_len(context.synthetic_caller())?.is_none() {
            return Err(Error::ContractDoesNotExist(
                context.synthetic_caller(),
            ));
        }

        let fn_arg = fn_arg.into();
        self.inner_mut().root_call_context = Some(context);
        let _context_guard = RootCallContextGuard { inner: self.inner };

        #[cfg(feature = "call-hook")]
        {
            // The hook sees the synthetic caller as the calling identity,
            // matching what the callee's caller/call-stack queries report.
            let caller = context.synthetic_caller();
            let (hook_result, hook_events) =
                self.call_hook(&caller, &contract, fn_name, &fn_arg);
            match hook_result {
                Ok(None) => {}
                Ok(Some(_)) => {
                    // A root call has no caller frame to charge the
                    // interception's gas against and no caller arg buffer to
                    // write its output into, so the hook cannot answer on the
                    // callee's behalf here. Refusing keeps that explicit
                    // rather than silently running the callee anyway.
                    self.revert_events_at(&hook_events);
                    return Err(Error::SessionError(
                        "root call context calls cannot be intercepted".into(),
                    ));
                }
                Err(err) => {
                    self.revert_events_at(&hook_events);
                    return Err(match err {
                        ContractError::Panic(msg) => Error::Panic(msg),
                        ContractError::OutOfGas => Error::OutOfGas,
                        ContractError::DoesNotExist => {
                            Error::ContractDoesNotExist(contract)
                        }
                        ContractError::Unknown => {
                            Error::Panic("unknown call-hook error".into())
                        }
                    });
                }
            }
        }

        self.call_raw(contract, fn_name, fn_arg, gas_limit)
    }

    /// Migrates a `contract` to a new `bytecode`, performing modifications to
    /// its state as specified by the closure.
    ///
    /// The closure takes a contract ID of where the new contract will be
    /// available during the migration, and a mutable reference to a session,
    /// allowing the caller to perform calls and other operations on the new
    /// (and old) contract.
    ///
    /// At the end of the migration, the new contract will be available at the
    /// given `contract` ID, and the old contract will be removed from the
    /// state.
    ///
    /// If the `owner` of a contract is not set, it will be set to the owner of
    /// the contract being replaced. If it is set, then it will be used as the
    /// new owner.
    ///
    /// # Errors
    /// The migration may error during execution for a myriad of reasons. The
    /// caller is encouraged to drop the `Session` should an error occur as it
    /// will more than likely be left in an inconsistent state.
    ///
    /// # Panics
    /// If the owner of the new contract is not set in `deploy_data`, and the
    /// contract being replaced does not exist, this will panic.
    pub fn migrate<'a, A, D, F>(
        mut self,
        contract: ContractId,
        bytecode: &[u8],
        deploy_data: D,
        deploy_gas_limit: u64,
        closure: F,
    ) -> Result<Self, Error>
    where
        A: 'a + for<'b> Serialize<StandardBufSerializer<'b>>,
        D: Into<ContractData<'a, A>>,
        F: FnOnce(ContractId, &mut Session) -> Result<(), Error>,
    {
        let mut new_contract_data = deploy_data.into();

        // If the contract being replaced exists, and the caller did not specify
        // an owner, set the owner to the owner of the contract being replaced.
        if let Some(old_contract_data) = self
            .inner_mut()
            .contract_session
            .contract(contract)
            .map_err(|err| PersistenceError(Arc::new(err)))?
        {
            if new_contract_data.owner.is_none() {
                new_contract_data.owner =
                    Some(old_contract_data.metadata.data().owner.clone());
            }
        }

        let (new_contract, _init_receipt) = self.deploy::<_, (), _>(
            bytecode,
            new_contract_data,
            deploy_gas_limit,
        )?;

        closure(new_contract, &mut self)?;

        self.inner_mut()
            .contract_session
            .replace(contract, new_contract)?;
        self.inner_mut().compiled_modules.remove(&contract);
        self.inner_mut().compiled_modules.remove(&new_contract);

        Ok(self)
    }

    /// Execute a *feeder* call on the current state of this session.
    ///
    /// Feeder calls are used to have the contract be able to report larger
    /// amounts of data to the host via the channel included in this call.
    ///
    /// These calls should be performed with a large amount of gas, since the
    /// contracts may spend quite a large amount in an effort to report data.
    pub fn feeder_call<A, R>(
        &mut self,
        contract: ContractId,
        fn_name: &str,
        fn_arg: &A,
        gas_limit: u64,
        feeder: mpsc::Sender<Vec<u8>>,
    ) -> Result<CallReceipt<R>, Error>
    where
        A: for<'b> Serialize<StandardBufSerializer<'b>>,
        A::Archived: for<'b> CheckBytes<DefaultValidator<'b>>,
        R: Archive,
        R::Archived: Deserialize<R, Infallible>
            + for<'b> CheckBytes<DefaultValidator<'b>>,
    {
        self.inner_mut().feeder = Some(feeder);
        let r = self.call(contract, fn_name, fn_arg, gas_limit);
        self.inner_mut().feeder = None;
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
        gas_limit: u64,
        feeder: mpsc::Sender<Vec<u8>>,
    ) -> Result<CallReceipt<Vec<u8>>, Error> {
        self.inner_mut().feeder = Some(feeder);
        let r = self.call_raw(contract, fn_name, fn_arg, gas_limit);
        self.inner_mut().feeder = None;
        r
    }

    /// Returns the current length of the memory of the given contract.
    ///
    /// If the contract does not exist, it will return `None`.
    pub fn memory_len(
        &mut self,
        contract_id: ContractId,
    ) -> Result<Option<usize>, Error> {
        Ok(self
            .inner_mut()
            .contract_session
            .contract(contract_id)
            .map_err(|err| PersistenceError(Arc::new(err)))?
            .map(|data| data.memory.current_len()))
    }

    pub(crate) fn instance(
        &mut self,
        contract_id: &ContractId,
    ) -> Option<&mut WrappedInstance> {
        self.inner_mut()
            .instances
            .get_mut(contract_id)
            .map(Box::as_mut)
    }

    fn clear_call_tree_and_instances(&mut self) {
        self.inner_mut().call_tree.clear();
        self.inner_mut().instances.clear();
    }

    /// Return the state root of the current state of the session.
    ///
    /// The state root is the root of a merkle tree whose leaves are the hashes
    /// of the state of of each contract, ordered by their contract ID.
    ///
    /// It also doubles as the ID of a commit - the commit root.
    pub fn root(&self) -> [u8; 32] {
        self.inner().contract_session.root().into()
    }

    /// Returns an iterator over the pages (and their indices) of a contract's
    /// memory, together with a proof of their inclusion in the state.
    ///
    /// The proof is a Merkle inclusion proof, and the caller is able to verify
    /// it by using [`verify`], and matching the root with the one returned by
    /// [`root`].
    ///
    /// [`verify`]: PageOpening::verify
    /// [`root`]: Session::root
    pub fn memory_pages(
        &self,
        contract: ContractId,
    ) -> Option<impl Iterator<Item = (usize, &[u8], PageOpening)>> {
        self.inner().contract_session.memory_pages(contract)
    }

    pub(crate) fn push_event(&mut self, event: Event) {
        self.inner_mut().events.push(event);
    }

    pub(crate) fn event_checkpoint(&self) -> usize {
        self.inner().events.len()
    }

    pub(crate) fn revert_events_from(&mut self, checkpoint: usize) {
        for event in self.inner_mut().events.iter_mut().skip(checkpoint) {
            event.reverted = true;
        }
    }

    /// Mark the events at exactly the given indices as reverted.
    #[cfg(feature = "call-hook")]
    pub(crate) fn revert_events_at(&mut self, indices: &[usize]) {
        let events = &mut self.inner_mut().events;
        for &index in indices {
            if let Some(event) = events.get_mut(index) {
                event.reverted = true;
            }
        }
    }

    pub(crate) fn push_feed(&mut self, data: Vec<u8>) -> Result<(), Error> {
        let feed = self.inner().feeder.as_ref().ok_or(Error::MissingFeed)?;
        feed.send(data).map_err(Error::FeedPulled)
    }

    fn new_instance(
        &mut self,
        contract_id: ContractId,
    ) -> Result<WrappedInstance, Error> {
        let store_data = self
            .inner_mut()
            .contract_session
            .contract(contract_id)
            .map_err(|err| PersistenceError(Arc::new(err)))?
            .ok_or(Error::ContractDoesNotExist(contract_id))?;

        let module = if let Some(module) =
            self.inner().compiled_modules.get(&contract_id)
        {
            module.clone()
        } else {
            let module = unsafe {
                WasmtimeModule::deserialize(
                    &self.engine,
                    store_data.module.serialize(),
                )?
            };
            self.inner_mut()
                .compiled_modules
                .insert(contract_id, module.clone());
            module
        };

        self.inner_mut().current = contract_id;

        let instance = WrappedInstance::new(
            self.clone(),
            contract_id,
            &module,
            store_data.memory,
        )?;

        Ok(instance)
    }

    pub(crate) fn host_query_arc(
        &self,
        name: &str,
    ) -> Option<Arc<dyn HostQuery>> {
        self.inner().host_queries.get_arc(name)
    }

    pub(crate) fn nth_from_top(&self, n: usize) -> Option<CallTreeElem> {
        self.inner().call_tree.nth_parent(n)
    }

    pub(crate) fn call_ids(&self) -> Vec<&ContractId> {
        self.inner().call_tree.call_ids()
    }

    pub(crate) fn effective_call_ids(&self) -> Vec<&ContractId> {
        let mut call_ids = self.call_ids();
        if let Some(context) = &self.inner().root_call_context {
            call_ids.push(&context.synthetic_caller);
        }
        call_ids
    }

    pub(crate) fn effective_caller(&self) -> Option<ContractId> {
        self.nth_from_top(1)
            .map(|elem| elem.contract_id)
            .or_else(|| {
                (self.inner().call_tree.depth() == 1)
                    .then(|| self.inner().root_call_context)
                    .flatten()
                    .map(RootCallContext::synthetic_caller)
            })
    }

    /// Creates a new instance of the given contract, returning its memory
    /// length.
    fn create_instance(
        &mut self,
        contract: ContractId,
    ) -> Result<usize, Error> {
        let instance = self.new_instance(contract)?;
        if self.inner().instances.contains_key(&contract) {
            panic!("Contract already in the stack: {contract:?}");
        }

        let mem_len = instance.mem_len();

        self.inner_mut()
            .instances
            .insert(contract, Box::new(instance));
        Ok(mem_len)
    }

    pub(crate) fn push_callstack(
        &mut self,
        contract_id: ContractId,
        limit: u64,
    ) -> Result<CallTreeElem, Error> {
        let current_depth = self.inner().call_tree.depth()
            + usize::from(self.inner().root_call_context.is_some());
        if current_depth >= MAX_CALL_DEPTH {
            return Err(Error::SessionError(
                format!("Maximum call depth exceeded ({MAX_CALL_DEPTH})")
                    .into(),
            ));
        }

        let mem_len =
            if let Some(instance) = self.inner().instances.get(&contract_id) {
                instance.mem_len()
            } else {
                self.create_instance(contract_id)?
            };

        self.inner_mut().call_tree.push(CallTreeElem {
            contract_id,
            limit,
            spent: 0,
            mem_len,
            instance_backed: true,
        });

        Ok(self
            .inner()
            .call_tree
            .nth_parent(0)
            .expect("We just pushed an element to the stack"))
    }

    pub(crate) fn move_up_call_tree(&mut self, spent: u64) {
        self.inner_mut().call_tree.move_up(spent);
    }

    pub(crate) fn move_up_prune_call_tree(&mut self) {
        self.inner_mut().call_tree.move_up_prune();
    }

    /// Push a lightweight frame onto the call tree without creating an
    /// instance.
    ///
    /// This is used when the call tree must record a contract that won't
    /// execute WASM — e.g. a call rerouted by the call-hook or the caller
    /// identity frame in [`call_as`](Session::call_as).
    pub(crate) fn push_callstack_frame(
        &mut self,
        contract_id: ContractId,
        limit: u64,
    ) -> Result<(), Error> {
        let current_depth = self.inner().call_tree.depth();
        if current_depth >= MAX_CALL_DEPTH {
            return Err(Error::SessionError(
                format!("Maximum call depth exceeded ({MAX_CALL_DEPTH})")
                    .into(),
            ));
        }

        self.inner_mut().call_tree.push(CallTreeElem {
            contract_id,
            limit,
            spent: 0,
            mem_len: 0,
            instance_backed: false,
        });

        Ok(())
    }

    pub(crate) fn revert_callstack(&mut self) -> Result<(), std::io::Error> {
        let call_tree: Vec<_> =
            self.inner().call_tree.iter().copied().collect();
        for elem in call_tree {
            // Lightweight frames have no instance to revert.
            if !elem.instance_backed {
                continue;
            }
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
    pub fn commit(mut self) -> Result<[u8; 32], Error> {
        self.inner_mut()
            .contract_session
            .commit()
            .map(Into::into)
            .map_err(|err| PersistenceError(Arc::new(err)))
    }

    #[cfg(feature = "debug")]
    pub(crate) fn register_debug<M: Into<String>>(&mut self, msg: M) {
        self.inner_mut().debug.push(msg.into());
    }

    pub fn with_debug<C, R>(&self, c: C) -> R
    where
        C: FnOnce(&[String]) -> R,
    {
        c(&self.inner().debug)
    }

    /// Returns the value of a metadata item.
    pub fn meta(&self, name: &str) -> Option<Vec<u8>> {
        self.inner().data.get(name)
    }

    /// Set the value of a metadata item.
    ///
    /// Returns the previous value of the metadata item.
    pub fn set_meta<S, V>(
        &mut self,
        name: S,
        value: V,
    ) -> Result<Option<Vec<u8>>, Error>
    where
        S: Into<Cow<'static, str>>,
        V: for<'a> Serialize<StandardBufSerializer<'a>>,
    {
        let data = Self::serialize_data(&value)?;
        Ok(self.inner_mut().data.set(name, data))
    }

    /// Remove a metadata item.
    ///
    /// Returns the value of the removed item (if any).
    pub fn remove_meta<S>(&mut self, name: S) -> Option<Vec<u8>>
    where
        S: Into<Cow<'static, str>>,
    {
        self.inner_mut().data.remove(name)
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

    fn revert_failed_call(
        &mut self,
        event_checkpoint: usize,
        err: Error,
    ) -> Error {
        let err = if let Err(io_err) = self.revert_callstack() {
            Error::MemorySnapshotFailure {
                reason: Some(Arc::new(err)),
                io: Arc::new(io_err),
            }
        } else {
            err
        };
        self.revert_events_from(event_checkpoint);
        self.move_up_prune_call_tree();
        self.clear_call_tree_and_instances();
        err
    }

    /// Execute a contract call, optionally with a specified caller identity.
    ///
    /// When `caller` is `Some`, a lightweight frame is pushed onto the call
    /// tree first so that `abi::caller()` inside the callee returns the
    /// specified identity and the call stack depth matches a normal
    /// contract-to-contract call.
    fn call_inner(
        &mut self,
        caller: Option<ContractId>,
        contract: ContractId,
        fname: &str,
        fdata: Vec<u8>,
        limit: u64,
    ) -> Result<(Vec<u8>, u64, CallTree), Error> {
        // Reject oversized arguments before any frame is pushed — a later
        // failure of the argument write would return with the frames and
        // snapshot still in place, corrupting the session for subsequent
        // calls.
        if fdata.len() > ARGBUF_LEN {
            return Err(Error::MemoryAccessOutOfBounds {
                offset: 0,
                len: fdata.len(),
                mem_len: ARGBUF_LEN,
            });
        }

        let event_checkpoint = self.event_checkpoint();

        if let Some(caller) = caller {
            self.push_callstack_frame(caller, limit)?;
        }

        let stack_element = match self.push_callstack(contract, limit) {
            Ok(elem) => elem,
            Err(err) => {
                // `clear` frees the whole tree regardless of the cursor
                // position, so the frames pushed above need no pruning.
                self.clear_call_tree_and_instances();
                return Err(err);
            }
        };
        {
            let instance = self
                .instance(&stack_element.contract_id)
                .expect("instance should exist");
            if let Err(err) = instance.snap() {
                self.clear_call_tree_and_instances();
                return Err(Error::MemorySnapshotFailure {
                    reason: None,
                    io: Arc::new(err),
                });
            }
        }

        let ret_len = {
            let instance = self
                .instance(&stack_element.contract_id)
                .expect("instance should exist");
            let arg_len = match instance.write_bytes_to_arg_buffer(&fdata) {
                Ok(arg_len) => arg_len,
                Err(err) => {
                    return Err(self.revert_failed_call(event_checkpoint, err));
                }
            };
            let ret_len = instance
                .call(fname, arg_len, limit)
                .map_err(Error::normalize)
                .map_err(|err| {
                    self.revert_failed_call(event_checkpoint, err)
                })?;
            ret_len as u32
        };

        let (ret, spent) = {
            let instance = self
                .instance(&stack_element.contract_id)
                .expect("instance should exist");
            if let Err(err) = check_arg(instance, ret_len) {
                return Err(self.revert_failed_call(event_checkpoint, err));
            };
            let ret = instance.read_bytes_from_arg_buffer(ret_len);
            let spent = limit - instance.get_remaining_gas();
            (ret, spent)
        };

        let call_tree: Vec<_> =
            self.inner().call_tree.iter().copied().collect();
        for elem in call_tree {
            // Lightweight frames have no instance to apply.
            if !elem.instance_backed {
                continue;
            }
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
        mem::swap(&mut self.inner_mut().call_tree, &mut call_tree);
        call_tree.update_spent(spent);

        self.clear_call_tree_and_instances();

        Ok((ret, spent, call_tree))
    }

    /// Execute a nested contract call with a specified caller identity.
    ///
    /// Unlike [`call_inner`], this is designed for calls made from within an
    /// active execution context (e.g. from a call-hook).  On error it reverts
    /// **only the callee's state** (via [`revert_callstack`] scoped to the
    /// callee subtree) without clearing the entire call tree or instance
    /// map — the outer call chain and any earlier successful nested calls
    /// are preserved.  Events emitted within the failed subtree are marked
    /// as reverted, mirroring the inter-contract-call error path.
    ///
    /// This matches the ICC error semantics in [`imports::c`]: a failed
    /// callee is reverted and pruned, and the error is returned to the
    /// caller.  The full-chain rollback happens higher up — when the hook
    /// propagates the error and the outer WASM caller fails, its own error
    /// path reverts the entire subtree (including frames from earlier
    /// successful `call_nested` calls within the same hook invocation).
    ///
    /// # Gas
    ///
    /// The nested call is paid for by the contract whose inter-contract call
    /// the hook intercepted (the top of the call tree when the hook fired):
    /// `gas_limit` resolves through [`callee_gas_limit`] against that
    /// contract's remaining gas — the inter-contract-call convention — and
    /// the gas spent by the nested call — the full resolved limit on
    /// failure, like a failed inter-contract call — is deducted from its
    /// fuel meter.  This keeps the call tree's invariant that a parent's
    /// spent gas covers the sum of its children's.
    ///
    /// `apply()` is intentionally not called here: `move_up_call_tree` keeps
    /// the nested frames in the tree as children of the outer caller, so the
    /// outer `call_inner`'s apply loop will process them when the top-level
    /// call completes.
    ///
    /// [`call_inner`]: Session::call_inner
    /// [`revert_callstack`]: Session::revert_callstack
    /// [`imports::c`]: crate::imports
    #[cfg(feature = "call-hook")]
    fn call_nested(
        &mut self,
        caller: ContractId,
        callee: ContractId,
        fn_name: &str,
        fn_args: &[u8],
        gas_limit: u64,
    ) -> Result<(Vec<u8>, u64), Error> {
        if fn_name == INIT_METHOD {
            return Err(InitalizationError("init call not allowed".into()));
        }

        if fn_args.len() > ARGBUF_LEN {
            return Err(Error::MemoryAccessOutOfBounds {
                offset: 0,
                len: fn_args.len(),
                mem_len: ARGBUF_LEN,
            });
        }

        // The suspended contract whose inter-contract call was intercepted
        // pays for the nested call. The hook only fires while that contract
        // executes WASM, so the top of the call tree is instance-backed.
        let outer_id = self
            .nth_from_top(0)
            .ok_or(Error::SessionError(
                "call_as requires an active contract call".into(),
            ))?
            .contract_id;
        let outer_remaining = self
            .instance(&outer_id)
            .ok_or(Error::SessionError(
                "suspended caller instance should exist".into(),
            ))?
            .get_remaining_gas();
        // Same limit resolution as a WASM inter-contract call: `0` or an
        // over-budget limit selects the `GAS_PASS_PCT` share of the
        // intercepted contract's remaining gas, keeping the reserve that
        // lets a nested failure propagate instead of starving the outer
        // frames into `OutOfGas`.
        let nested_limit = callee_gas_limit(outer_remaining, gas_limit);

        let event_checkpoint = self.event_checkpoint();

        self.push_callstack_frame(caller, nested_limit)?;

        let stack_element = match self.push_callstack(callee, nested_limit) {
            Ok(elem) => elem,
            Err(err) => {
                self.move_up_prune_call_tree();
                return Err(err);
            }
        };

        // The snapshot is taken outside the closure so its failure does not
        // reach the revert below — an unmatched revert would roll memory
        // back past this call's boundary (to a previous snapshot or the
        // base state).
        let snapped = match self.instance(&stack_element.contract_id) {
            Some(instance) => {
                instance.snap().map_err(|err| Error::MemorySnapshotFailure {
                    reason: None,
                    io: Arc::new(err),
                })
            }
            None => {
                Err(Error::SessionError("callee instance should exist".into()))
            }
        };
        if let Err(err) = snapped {
            self.move_up_prune_call_tree();
            self.move_up_prune_call_tree();
            // A failed nested call charges its full limit, exactly like
            // a failed inter-contract call.
            if let Some(outer_instance) = self.instance(&outer_id) {
                outer_instance
                    .set_remaining_gas(outer_remaining - nested_limit);
            }
            return Err(err);
        }

        let result = (|| -> Result<(Vec<u8>, u64), Error> {
            let instance = self.instance(&stack_element.contract_id).ok_or(
                Error::SessionError("callee instance should exist".into()),
            )?;

            let arg_len = instance.write_bytes_to_arg_buffer(fn_args)?;
            let ret_len = instance
                .call(fn_name, arg_len, nested_limit)
                .map_err(Error::normalize)?;
            check_arg(instance, ret_len as u32)?;

            let ret = instance.read_bytes_from_arg_buffer(ret_len as u32);
            let spent = nested_limit - instance.get_remaining_gas();

            Ok((ret, spent))
        })();

        match result {
            Ok((ret, spent)) => {
                self.move_up_call_tree(spent);
                // The lightweight caller frame must report `spent` so
                // that `CallTree::update_spent` can subtract the
                // callee child's `spent` and arrive at zero — the
                // frame itself did no work.
                self.move_up_call_tree(spent);
                // Charge the nested gas to the suspended caller's fuel
                // meter. This also restores that contract's fuel when the
                // nested callee was the same contract (shared instance).
                if let Some(outer_instance) = self.instance(&outer_id) {
                    outer_instance.set_remaining_gas(outer_remaining - spent);
                }
                Ok((ret, spent))
            }
            Err(mut err) => {
                if let Err(io_err) = self.revert_callstack() {
                    err = Error::MemorySnapshotFailure {
                        reason: Some(Arc::new(err)),
                        io: Arc::new(io_err),
                    };
                }
                self.revert_events_from(event_checkpoint);
                self.move_up_prune_call_tree();
                self.move_up_prune_call_tree();
                // A failed nested call charges its full limit, exactly like
                // a failed inter-contract call.
                if let Some(outer_instance) = self.instance(&outer_id) {
                    outer_instance
                        .set_remaining_gas(outer_remaining - nested_limit);
                }
                Err(err)
            }
        }
    }

    pub fn contract_metadata(
        &mut self,
        contract_id: &ContractId,
    ) -> Option<&ContractMetadata> {
        self.inner_mut()
            .contract_session
            .contract_metadata(contract_id)
    }

    /// Set a hook that is called before each inter-contract call and
    /// contextual root call.
    ///
    /// See [`CallHook`] for the full signature and return semantics.
    #[cfg(feature = "call-hook")]
    pub fn set_call_hook(&mut self, hook: CallHook) -> Option<CallHook> {
        self.inner_mut().call_hook.replace(hook)
    }

    /// Remove the call hook, if one is set.
    #[cfg(feature = "call-hook")]
    pub fn clear_call_hook(&mut self) -> Option<CallHook> {
        self.inner_mut().call_hook.take()
    }

    /// Run the call hook, if one is set.
    ///
    /// Returns the hook's result — `Ok(None)` if the call should proceed
    /// normally (or no hook is set), `Ok(Some(interception))` if the hook
    /// handled the call, or `Err(contract_error)` if the hook rejects —
    /// together with the indices of events the hook emitted through its
    /// [`HookContext`], so the caller can mark exactly those events reverted
    /// when the call does not go through.
    ///
    /// The hook is cloned out of `SessionInner` before it is invoked, so no
    /// borrow of the inner session is held while the `HookContext` mutates
    /// it re-entrantly.
    #[cfg(feature = "call-hook")]
    pub(crate) fn call_hook(
        &self,
        caller: &ContractId,
        callee: &ContractId,
        fn_name: &str,
        arg: &[u8],
    ) -> (Result<Option<Interception>, ContractError>, Vec<usize>) {
        let hook = match &self.inner().call_hook {
            Some(hook) => hook.clone(),
            None => return (Ok(None), Vec::new()),
        };
        let mut ctx = HookContext {
            session: self.clone(),
            emitted: Vec::new(),
        };
        let result = hook(caller, callee, fn_name, arg, &mut ctx);
        (result, ctx.emitted)
    }
}

/// The receipt given for a call execution using one of either [`call`] or
/// [`call_raw`]. This receipt is also returned within a tuple when
/// a Contract is deployed and its `init` method is executed.
///
/// [`call`]: [`Session::call`]
/// [`call_raw`]: [`Session::call_raw`]
#[derive(Debug)]
pub struct CallReceipt<T> {
    /// The amount of gas spent in the execution of the call.
    pub gas_spent: u64,
    /// The limit used in during this execution.
    pub gas_limit: u64,

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
            gas_spent: self.gas_spent,
            gas_limit: self.gas_limit,
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
    excluded_host_queries: BTreeSet<String>,
}

impl SessionData {
    pub fn builder() -> SessionDataBuilder {
        SessionDataBuilder {
            data: BTreeMap::new(),
            base: None,
            excluded_host_queries: BTreeSet::new(),
        }
    }

    fn get(&self, name: &str) -> Option<Vec<u8>> {
        self.data.get(name).cloned()
    }

    fn set<S>(&mut self, name: S, data: Vec<u8>) -> Option<Vec<u8>>
    where
        S: Into<Cow<'static, str>>,
    {
        self.data.insert(name.into(), data)
    }

    fn remove<S>(&mut self, name: S) -> Option<Vec<u8>>
    where
        S: Into<Cow<'static, str>>,
    {
        self.data.remove(&name.into())
    }

    pub fn excluded_host_queries(&self) -> Iter<String> {
        self.excluded_host_queries.iter()
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
    excluded_host_queries: BTreeSet<String>,
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

    pub fn exclude_hq(mut self, name: String) -> Self {
        self.excluded_host_queries.insert(name);
        self
    }

    fn build(&self) -> SessionData {
        SessionData {
            data: self.data.clone(),
            base: self.base,
            excluded_host_queries: self.excluded_host_queries.clone(),
        }
    }
}
