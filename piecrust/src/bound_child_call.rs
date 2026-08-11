// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use piecrust_uplink::ContractId;

use crate::{Error, HOST_CALL_FRAME_MAX_LEN};

/// One exact direct-child call bound to a root invocation.
#[derive(Debug)]
pub struct BoundChildCall {
    callee: ContractId,
    expected_method: String,
    expected_argument: Vec<u8>,
    delivered_method: String,
    delivered_argument: Vec<u8>,
}

pub(crate) struct BoundChildCallDelivery {
    pub(crate) method: String,
    pub(crate) argument: Vec<u8>,
}

pub(crate) struct PendingBoundChildCall {
    root_caller: ContractId,
    call: BoundChildCall,
    state: BoundChildCallState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BoundChildCallState {
    Available,
    InFlight,
    Resolved,
    Poisoned,
}

impl BoundChildCall {
    /// Bind one expected direct-child invocation to delivered B call data.
    pub fn new(
        callee: ContractId,
        expected_method: impl Into<String>,
        expected_argument: Vec<u8>,
        delivered_method: impl Into<String>,
        delivered_argument: Vec<u8>,
    ) -> Result<Self, Error> {
        validate_argument(&expected_argument)?;
        validate_argument(&delivered_argument)?;
        Ok(Self {
            callee,
            expected_method: validate_method(expected_method.into())?,
            expected_argument,
            delivered_method: validate_method(delivered_method.into())?,
            delivered_argument,
        })
    }

    /// Return the exact argument expected from the root contract.
    pub fn expected_argument(&self) -> &[u8] {
        &self.expected_argument
    }

    pub(crate) const fn bind_root(
        self,
        root_caller: ContractId,
    ) -> PendingBoundChildCall {
        PendingBoundChildCall {
            root_caller,
            call: self,
            state: BoundChildCallState::Available,
        }
    }
}

impl PendingBoundChildCall {
    pub(crate) fn resolve(
        &mut self,
        caller: ContractId,
        call_depth: usize,
        callee: ContractId,
        method: &str,
        argument: &[u8],
    ) -> Result<Option<BoundChildCallDelivery>, Error> {
        if caller != self.root_caller || call_depth != 1 {
            return Ok(None);
        }
        if callee != self.call.callee || method != self.call.expected_method {
            return Ok(None);
        }
        if argument != self.call.expected_argument {
            self.state = BoundChildCallState::Poisoned;
            return Err(Error::SessionError(
                "bound child-call argument does not match".into(),
            ));
        }
        if self.state != BoundChildCallState::Available {
            let message = match self.state {
                BoundChildCallState::Available => unreachable!(),
                BoundChildCallState::InFlight => {
                    "bound child call is already in flight"
                }
                BoundChildCallState::Resolved => {
                    "bound child call was already resolved"
                }
                BoundChildCallState::Poisoned => {
                    "bound child-call delivery is poisoned"
                }
            };
            self.state = BoundChildCallState::Poisoned;
            return Err(Error::SessionError(message.into()));
        }

        self.state = BoundChildCallState::InFlight;
        Ok(Some(BoundChildCallDelivery {
            method: core::mem::take(&mut self.call.delivered_method),
            argument: core::mem::take(&mut self.call.delivered_argument),
        }))
    }

    pub(crate) fn resolve_delivery(&mut self) {
        if self.state == BoundChildCallState::InFlight {
            self.state = BoundChildCallState::Resolved;
        }
    }

    pub(crate) fn poison_delivery(&mut self) {
        self.state = BoundChildCallState::Poisoned;
    }

    pub(crate) fn ensure_resolved(&self) -> Result<(), Error> {
        if self.state == BoundChildCallState::Resolved {
            Ok(())
        } else {
            Err(Error::SessionError(
                "bound child call did not resolve exactly once".into(),
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

fn validate_argument(argument: &[u8]) -> Result<(), Error> {
    if argument.len() > HOST_CALL_FRAME_MAX_LEN {
        return Err(Error::ArgumentBufferOverflow {
            len: argument.len(),
            max_len: HOST_CALL_FRAME_MAX_LEN,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call() -> BoundChildCall {
        BoundChildCall::new(
            ContractId::from_bytes([2; 32]),
            "dispatch",
            vec![3; 32],
            "run",
            vec![4; 64],
        )
        .unwrap()
    }

    #[test]
    fn mismatch_and_duplicate_attempts_poison_delivery() {
        let caller = ContractId::from_bytes([1; 32]);
        let callee = ContractId::from_bytes([2; 32]);
        let mut pending = call().bind_root(caller);

        assert!(
            pending
                .resolve(caller, 1, callee, "dispatch", &[9; 32])
                .is_err()
        );
        assert!(
            pending
                .resolve(caller, 1, callee, "dispatch", &[3; 32])
                .is_err()
        );

        let mut pending = call().bind_root(caller);
        assert!(
            pending
                .resolve(caller, 1, callee, "dispatch", &[3; 32])
                .unwrap()
                .is_some()
        );
        pending.resolve_delivery();
        assert!(
            pending
                .resolve(caller, 1, callee, "dispatch", &[3; 32])
                .is_err()
        );
        assert!(pending.ensure_resolved().is_err());
    }

    #[test]
    fn empty_delivered_method_remains_contract_visible() {
        let caller = ContractId::from_bytes([1; 32]);
        let callee = ContractId::from_bytes([2; 32]);
        let mut pending = BoundChildCall::new(
            callee,
            "dispatch",
            vec![3; 32],
            "",
            vec![4; 64],
        )
        .unwrap()
        .bind_root(caller);

        let delivery = pending
            .resolve(caller, 1, callee, "dispatch", &[3; 32])
            .unwrap()
            .expect("the exact child call should be delivered");
        assert!(delivery.method.is_empty());
    }
}
