// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust_uplink::ContractId;

use crate::{Error, HOST_CALL_FRAME_MAX_LEN};

const DESCRIPTOR_DOMAIN: &[u8; 16] = b"piecrust-call-v1";

/// A host-owned call payload committed to one callee and method.
#[derive(Debug)]
pub struct CommittedCall {
    callee: ContractId,
    dispatch_method: String,
    method: String,
    dispatch_argument: Vec<u8>,
    payload: Vec<u8>,
}

pub(crate) struct CommittedCallDelivery {
    pub(crate) method: String,
    pub(crate) payload: Vec<u8>,
}

pub(crate) struct PendingCommittedCall {
    root_caller: ContractId,
    call: CommittedCall,
    state: CommittedCallState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommittedCallState {
    Available,
    InFlight,
    Resolved,
    Poisoned,
}

impl CommittedCall {
    /// Commit a bounded payload to an exact callee and method.
    pub fn new(
        callee: ContractId,
        method: impl Into<String>,
        payload: Vec<u8>,
    ) -> Result<Self, Error> {
        if payload.len() > HOST_CALL_FRAME_MAX_LEN {
            return Err(Error::ArgumentBufferOverflow {
                len: payload.len(),
                max_len: HOST_CALL_FRAME_MAX_LEN,
            });
        }

        let method = validate_method(method.into())?;
        let dispatch_argument =
            default_dispatch_argument(callee, &method, &method, &payload);
        Ok(Self {
            callee,
            dispatch_method: method.clone(),
            method,
            dispatch_argument,
            payload,
        })
    }

    /// Dispatch the committed call through a compact intermediary method.
    ///
    /// The direct child invocation must use `dispatch_method` and the compact
    /// descriptor. The callee and call hook receive the method and payload
    /// originally passed to [`Self::new`].
    pub fn dispatch_via(
        mut self,
        dispatch_method: impl Into<String>,
        dispatch_argument: impl Into<Vec<u8>>,
    ) -> Result<Self, Error> {
        self.dispatch_method = validate_method(dispatch_method.into())?;
        self.dispatch_argument = dispatch_argument.into();
        Ok(self)
    }

    /// Return the compact argument passed through the root contract.
    pub fn dispatch_argument(&self) -> &[u8] {
        &self.dispatch_argument
    }

    pub(crate) const fn bind_root(
        self,
        root_caller: ContractId,
    ) -> PendingCommittedCall {
        PendingCommittedCall {
            root_caller,
            call: self,
            state: CommittedCallState::Available,
        }
    }
}

impl PendingCommittedCall {
    pub(crate) fn resolve(
        &mut self,
        caller: ContractId,
        call_depth: usize,
        callee: ContractId,
        method: &str,
        descriptor: &[u8],
    ) -> Result<Option<CommittedCallDelivery>, Error> {
        if caller != self.root_caller || call_depth != 1 {
            return Ok(None);
        }
        if callee != self.call.callee || method != self.call.dispatch_method {
            return Ok(None);
        }
        if descriptor != self.call.dispatch_argument {
            self.state = CommittedCallState::Poisoned;
            return Err(Error::SessionError(
                "committed call descriptor does not match its payload".into(),
            ));
        }
        if self.state != CommittedCallState::Available {
            let message = match self.state {
                CommittedCallState::Available => unreachable!(),
                CommittedCallState::InFlight => {
                    "committed call payload is already in flight"
                }
                CommittedCallState::Resolved => {
                    "committed call payload was already resolved"
                }
                CommittedCallState::Poisoned => {
                    "committed call payload delivery is poisoned"
                }
            };
            self.state = CommittedCallState::Poisoned;
            return Err(Error::SessionError(message.into()));
        }

        self.state = CommittedCallState::InFlight;
        Ok(Some(CommittedCallDelivery {
            method: core::mem::take(&mut self.call.method),
            payload: core::mem::take(&mut self.call.payload),
        }))
    }

    pub(crate) fn resolve_delivery(&mut self) {
        if self.state == CommittedCallState::InFlight {
            self.state = CommittedCallState::Resolved;
        }
    }

    pub(crate) fn poison_delivery(&mut self) {
        self.state = CommittedCallState::Poisoned;
    }

    pub(crate) fn ensure_resolved(&self) -> Result<(), Error> {
        if self.state == CommittedCallState::Resolved {
            Ok(())
        } else {
            Err(Error::SessionError(
                "committed call payload did not resolve exactly once".into(),
            ))
        }
    }
}

fn validate_method(method: String) -> Result<String, Error> {
    if u32::try_from(method.len()).is_err() {
        return Err(Error::InvalidFunction(method));
    }
    Ok(method)
}

fn default_dispatch_argument(
    callee: ContractId,
    dispatch_method: &str,
    method: &str,
    payload: &[u8],
) -> Vec<u8> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(DESCRIPTOR_DOMAIN);
    hasher.update(callee.as_bytes());
    hasher.update(&(dispatch_method.len() as u32).to_le_bytes());
    hasher.update(dispatch_method.as_bytes());
    hasher.update(&(method.len() as u32).to_le_bytes());
    hasher.update(method.as_bytes());
    hasher.update(&(payload.len() as u32).to_le_bytes());
    hasher.update(payload);

    let mut descriptor = vec![0; 52];
    descriptor[..16].copy_from_slice(DESCRIPTOR_DOMAIN);
    descriptor[16..20].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    descriptor[20..].copy_from_slice(hasher.finalize().as_bytes());
    descriptor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_binds_callee_method_length_and_payload() {
        let callee = ContractId::from_bytes([1; 32]);
        let base = CommittedCall::new(callee, "run", vec![2; 32]).unwrap();
        let changed_callee = CommittedCall::new(
            ContractId::from_bytes([3; 32]),
            "run",
            vec![2; 32],
        )
        .unwrap();
        let changed_method =
            CommittedCall::new(callee, "step", vec![2; 32]).unwrap();
        let changed_dispatch = CommittedCall::new(callee, "run", vec![2; 32])
            .unwrap()
            .dispatch_via("dispatch", vec![9; 52])
            .unwrap();
        let changed_payload =
            CommittedCall::new(callee, "run", vec![4; 32]).unwrap();
        let changed_length =
            CommittedCall::new(callee, "run", vec![2; 31]).unwrap();

        assert_ne!(
            base.dispatch_argument(),
            changed_callee.dispatch_argument()
        );
        assert_ne!(
            base.dispatch_argument(),
            changed_method.dispatch_argument()
        );
        assert_ne!(
            base.dispatch_argument(),
            changed_dispatch.dispatch_argument()
        );
        assert_ne!(
            base.dispatch_argument(),
            changed_payload.dispatch_argument()
        );
        assert_ne!(
            base.dispatch_argument(),
            changed_length.dispatch_argument()
        );
        assert_eq!(&base.dispatch_argument()[..16], DESCRIPTOR_DOMAIN);
    }

    #[test]
    fn mismatch_and_duplicate_attempts_poison_delivery() {
        let caller = ContractId::from_bytes([1; 32]);
        let callee = ContractId::from_bytes([2; 32]);
        let call = CommittedCall::new(callee, "run", vec![3; 32]).unwrap();
        let descriptor = call.dispatch_argument().to_vec();
        let mut pending = call.bind_root(caller);
        let mut wrong = descriptor.clone();
        wrong[0] ^= 1;

        assert!(pending.resolve(caller, 1, callee, "run", &wrong).is_err());
        assert!(
            pending
                .resolve(caller, 1, callee, "run", &descriptor)
                .is_err()
        );
        assert!(pending.ensure_resolved().is_err());

        let call = CommittedCall::new(callee, "run", vec![3; 32]).unwrap();
        let descriptor = call.dispatch_argument().to_vec();
        let mut pending = call.bind_root(caller);
        assert!(
            pending
                .resolve(caller, 1, callee, "run", &descriptor)
                .unwrap()
                .is_some()
        );
        pending.resolve_delivery();
        assert!(
            pending
                .resolve(caller, 1, callee, "run", &descriptor)
                .is_err()
        );
        assert!(pending.ensure_resolved().is_err());
    }

    #[test]
    fn empty_committed_method_remains_a_contract_visible_call() {
        let callee = ContractId::from_bytes([2; 32]);
        let call = CommittedCall::new(callee, "", vec![3; 32]).unwrap();
        let descriptor = call.dispatch_argument().to_vec();
        let mut pending = call.bind_root(ContractId::from_bytes([1; 32]));

        let delivery = pending
            .resolve(
                ContractId::from_bytes([1; 32]),
                1,
                callee,
                "",
                &descriptor,
            )
            .unwrap()
            .expect("the exact empty method should be delivered");
        assert!(delivery.method.is_empty());
    }
}
