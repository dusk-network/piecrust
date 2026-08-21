// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! The call hook: the types a host implements it with, and the arbitration
//! the VM runs it through before an inter-contract call.

use std::sync::Arc;

use piecrust_uplink::{ARGBUF_LEN, ContractError, ContractId, Event};

use crate::Error;
use crate::imports::callee_gas_limit;
use crate::instance::Env;
use crate::session::{INIT_METHOD, Session};

/// The result of a call-hook that intercepted a call on behalf of the VM.
///
/// When a [`CallHook`] returns `Ok(Some(Interception { .. }))`, the VM skips
/// WASM execution, writes `output` to the arg buffer as the call return, and
/// charges `gas_spent` against the caller's remaining gas.
///
/// The intercepted callee is recorded in the call tree but is never
/// instantiated — the VM does not verify that it exists, so a hook can
/// answer calls on behalf of contracts that are not deployed.
pub struct Interception {
    /// The serialized return value to write to the arg buffer.
    ///
    /// Must fit `ARGBUF_LEN`. A longer output fails the call and charges the
    /// caller the full callee limit, like any failed callee.
    pub output: Vec<u8>,
    /// Gas to charge the caller for this intercepted call.
    ///
    /// Capped by the callee limit — the same inter-contract-call limit
    /// resolution [`HookContext::call_as_raw`] describes, not the caller's
    /// whole remaining gas. A larger charge is treated as the callee
    /// running out of gas: the call fails with `OutOfGas` and the caller
    /// is charged the full callee limit.
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
/// [`call_as_raw`](HookContext::call_as_raw) executes a contract call with a
/// specified caller identity.  Gas accounting, call stack management,
/// and state rollback on failure are handled by the VM.
///
/// Re-entrancy works naturally: if the called contract makes another
/// inter-contract call, the hook fires again with a fresh `HookContext`.
///
/// # Rollback semantics
///
/// When a [`call_as_raw`](HookContext::call_as_raw) call fails, **only that
/// callee's state** is reverted — earlier successful `call_as_raw` mutations
/// within the same hook invocation are preserved in the session.
///
/// However, the hook is expected to propagate the error by returning
/// `Err(contract_error)`.  This surfaces as the same [`ContractError`] to
/// the outer WASM caller (the contract whose ICC was intercepted).  If the
/// outer caller
/// does not handle the error (e.g. it calls `.unwrap()`), it panics and
/// the normal ICC error path in the VM reverts the **entire call subtree**
/// — including any mutations made by earlier successful `call_as_raw` calls
/// in the hook.
///
/// In short: individual `call_as_raw` failures are scoped, but unhandled
/// errors cascade through the WASM call chain and revert everything,
/// matching the transaction semantics of the contract execution path.
///
/// # Events
///
/// [`emit`](HookContext::emit) pushes events into the session's event
/// stream in execution order, interleaved with events from WASM
/// contracts — not appended at the end.
pub struct HookContext {
    session: Session,
    /// Indices of events pushed through [`HookContext::emit`], so that
    /// exactly these — and not events of successful nested calls, whose
    /// state persists — can be marked reverted when the hook rejects or
    /// its interception fails.
    emitted: Vec<usize>,
}

impl HookContext {
    pub(crate) fn new(session: Session) -> Self {
        Self {
            session,
            emitted: Vec::new(),
        }
    }

    /// Consume the context, returning the indices of the events it emitted.
    pub(crate) fn into_emitted(self) -> Vec<usize> {
        self.emitted
    }

    /// Return the current call stack, ordered like `abi::callstack()`: index
    /// 0 is the immediate caller, and the last element is the root contract
    /// call.
    ///
    /// The callee whose call triggered the hook is not included, because the
    /// hook runs before the callee is pushed.  Caller identity frames are
    /// included — those pushed by [`Session::call_as`] and those pushed by
    /// [`call_as_raw`](HookContext::call_as_raw), which outlive their nested
    /// call — as is the synthetic caller of a contextual root call.
    ///
    /// An asserted identity is indistinguishable from a contract that
    /// actually executed: both are plain [`ContractId`]s here.  A hook may
    /// therefore *deny* on an ancestor safely, but must not *grant* on one —
    /// any earlier `call_as_raw` in the chain can put an arbitrary identity
    /// into this list.
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
    /// checks must therefore apply them to its own `call_as_raw` calls itself.
    ///
    /// The nested call is paid for by the contract whose call the hook
    /// intercepted, and `gas_limit` follows the inter-contract-call
    /// convention: an explicit positive limit below that contract's
    /// remaining gas is used as-is, while `0` or an over-budget limit
    /// selects the default share of the remaining gas. The held-back
    /// reserve keeps a *failed* nested call — which charges its full
    /// limit, exactly like a failed inter-contract call — from starving
    /// the outer frames into `OutOfGas`.
    ///
    /// On error, the callee's state is reverted and the error is returned.
    /// State changes from earlier successful `call_as_raw` calls in the same
    /// hook invocation remain in the session until the outer call chain
    /// either commits or reverts them — see [rollback
    /// semantics](HookContext#rollback-semantics) on `HookContext`.
    pub fn call_as_raw(
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

/// What the hook decided about an inter-contract call.
///
/// The caller's fuel meter and argument buffer are left untouched: writing
/// the outcome into the live caller frame is the business of the import that
/// asked for the decision. Gas is reported as the caller's *resulting*
/// remaining gas rather than as a charge to subtract, so the deduction — and
/// with it the risk of an underflow wrapping into a near-maximal fuel value —
/// stays next to the bounds that make it sound.
pub(crate) enum Decision {
    /// No hook is set, or the hook allowed the call: execute the callee.
    Proceed,
    /// The hook answered the call itself: `output` is the callee's return
    /// value and `remaining` the caller's gas once the interception is paid
    /// for.
    Answered { output: Vec<u8>, remaining: u64 },
    /// The call never reaches the callee: `error` is surfaced to the caller
    /// as the call's result. `remaining` is `Some(gas)` where a failed callee
    /// would have charged its full limit, and `None` where the rejection is
    /// free of gas and the meter is left alone.
    Rejected {
        error: ContractError,
        remaining: Option<u64>,
    },
}

impl Decision {
    fn rejected(err: Error, remaining: Option<u64>) -> Self {
        Self::Rejected {
            error: ContractError::from(err),
            remaining,
        }
    }
}

/// Run the call hook for an inter-contract call and rule on its outcome.
///
/// Everything the decision implies for the call tree and the session's event
/// stream is done here; what it implies for the caller's fuel meter and
/// argument buffer is reported back through [`Decision`].
pub(crate) fn arbitrate(
    env: &mut Env,
    callee: ContractId,
    fn_name: &str,
    arg: &[u8],
    gas_limit: u64,
) -> Decision {
    let caller = env.self_contract_id().to_owned();

    // Events of successful nested `call_as_raw` calls stay live even when the
    // hook fails afterwards — their state mutations persist. Only the events
    // the hook emitted itself, tracked by index, are marked reverted when the
    // call does not go through.
    let (hook_result, hook_events) =
        env.call_hook(&caller, &callee, fn_name, arg);

    let interception = match hook_result {
        Ok(None) => return Decision::Proceed,
        Ok(Some(interception)) => interception,
        Err(c_err) => {
            env.revert_events_at(&hook_events);
            // Surface the hook's `ContractError` to the caller exactly as a
            // failed WASM callee would, preserving the variant the hook chose
            // (`Panic`, `Unknown`, ...) rather than flattening every
            // rejection to `Panic`.
            return Decision::Rejected {
                error: truncate_panic(c_err),
                remaining: None,
            };
        }
    };

    // The init prohibition is enforced during WASM execution, which an
    // interception skips — reject here so a hook cannot fabricate a
    // successful `init` call. Unlike the normal path, whose init failure
    // charges the callee limit, this rejection is free of gas — an accepted
    // divergence, since intercepting `init` is always a host-side bug.
    if fn_name == INIT_METHOD {
        env.revert_events_at(&hook_events);
        return Decision::rejected(
            Error::InitalizationError("init call not allowed".into()),
            None,
        );
    }

    // Gas is read after the hook ran: nested `call_as_raw` calls have already
    // been charged to this contract's fuel meter, and
    // `Interception::gas_spent` is an additional charge on top.
    let caller_remaining = env.self_instance().get_remaining_gas();
    let callee_limit = callee_gas_limit(caller_remaining, gas_limit);

    // `callee_gas_limit` never resolves above the caller's remaining gas, so
    // charging the full limit cannot underflow.
    let remaining_after_full_limit = caller_remaining - callee_limit;

    // An output that cannot fit the argument buffer fails the call: charge
    // the full limit, like any failed callee.
    if interception.output.len() > ARGBUF_LEN {
        env.revert_events_at(&hook_events);
        return Decision::rejected(
            Error::MemoryAccessOutOfBounds {
                offset: 0,
                len: interception.output.len(),
                mem_len: ARGBUF_LEN,
            },
            Some(remaining_after_full_limit),
        );
    }

    // A gas charge beyond what a real callee could have spent is treated as
    // the callee running out of gas: charge the full limit and surface
    // `OutOfGas`, like the normal call path.
    if interception.gas_spent > callee_limit {
        env.revert_events_at(&hook_events);
        return Decision::rejected(
            Error::OutOfGas,
            Some(remaining_after_full_limit),
        );
    }

    // Push/pop the callee on the call tree so the call is tracked. Use the
    // lightweight push — no instance is needed since the hook already
    // produced the result.
    if let Err(err) = env.push_callstack_frame(callee, callee_limit) {
        // No gas is charged, mirroring a failed `push_callstack` on the
        // normal path.
        env.revert_events_at(&hook_events);
        return Decision::rejected(err, None);
    }
    env.move_up_call_tree(interception.gas_spent);

    Decision::Answered {
        output: interception.output,
        // Guarded by the `gas_spent > callee_limit` rejection above, and
        // `callee_limit <= caller_remaining`.
        remaining: caller_remaining - interception.gas_spent,
    }
}

/// A panic message is an arbitrary host-side string, while
/// [`ContractError::to_parts`] needs 4 bytes of the argument buffer for
/// itself — truncate on a char boundary so the write cannot overrun the
/// buffer.
fn truncate_panic(err: ContractError) -> ContractError {
    match err {
        ContractError::Panic(msg) if msg.len() > ARGBUF_LEN - 4 => {
            let mut cut = ARGBUF_LEN - 4;
            while !msg.is_char_boundary(cut) {
                cut -= 1;
            }
            ContractError::Panic(msg[..cut].into())
        }
        other => other,
    }
}
