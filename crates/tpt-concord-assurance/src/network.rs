// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Networking assurance infrastructure (spec §12).
//!
//! Grounding: `tpt-solutions/tpt-pulse` — session-typed channels with
//! compile-time reactive signals — and Keystone's hand-written wire codecs
//! (protocol conformance lives in the keystone module).
//!
//! Claim modeled here: a **session-typed** channel admits exactly the
//! message sequences its type prescribes — send/recv ordering is a type
//! property, and violations are rejected at conformance time rather than
//! surfacing as runtime deadlocks. A half-closed connection refuses sends
//! but still drains buffered receives (TCP-style shutdown semantics).

use std::collections::BTreeSet;
use tpt_concord_core::{
    AssuranceGraph, AssuranceLevel, Claim, EvidenceKind, EvidenceRecord, Obligation, Scope,
};
use tpt_concord_model::{DeterministicMachine, StepOutcome};

/// Session actions for a simple `!T.^T` (send one, receive one) session.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SessionAction {
    Send(String),
    Recv(String),
    Close,
}

impl std::fmt::Display for SessionAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionAction::Send(m) => write!(f, "send({m})"),
            SessionAction::Recv(m) => write!(f, "recv({m})"),
            SessionAction::Close => f.write_str("close"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChannelState {
    /// Messages sent but not yet received (the in-flight buffer).
    pub buffered: Vec<String>,
    /// Whether the sending side has closed.
    pub send_closed: bool,
    /// Whether the channel is fully finished.
    pub finished: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SessionError {
    #[error("send on a closed channel")]
    SendAfterClose,
    #[error("expected to receive `{expected}`, found `{found}` (session type violation)")]
    TypeMismatch { expected: String, found: String },
    #[error("receive on empty channel would block (deadlock)")]
    WouldBlock,
    #[error("channel already finished")]
    Finished,
}

/// Session-typed channel model for a strict `send then receive` protocol:
/// each `Recv` must consume the message in flight — receiving anything else
/// is a *type* violation, and receiving with nothing in flight would deadlock.
#[derive(Debug, Clone, Default)]
pub struct SessionChannelModel;

impl DeterministicMachine for SessionChannelModel {
    type State = ChannelState;
    type Input = SessionAction;
    type Error = SessionError;

    fn initial_state(&self) -> ChannelState {
        ChannelState {
            buffered: Vec::new(),
            send_closed: false,
            finished: false,
        }
    }

    fn step(
        &self,
        s: &ChannelState,
        action: &SessionAction,
    ) -> Result<StepOutcome<ChannelState>, SessionError> {
        if s.finished {
            return Err(SessionError::Finished);
        }
        let mut next = s.clone();
        let effects = match action {
            SessionAction::Send(msg) => {
                if s.send_closed {
                    return Err(SessionError::SendAfterClose);
                }
                next.buffered.push(msg.clone());
                BTreeSet::from(["pulse.channel.send".to_string()])
            }
            SessionAction::Recv(expected) => {
                let Some(in_flight) = s.buffered.first() else {
                    return Err(SessionError::WouldBlock);
                };
                if in_flight != expected {
                    return Err(SessionError::TypeMismatch {
                        expected: expected.clone(),
                        found: in_flight.clone(),
                    });
                }
                next.buffered.remove(0);
                // A closed channel with nothing in flight is finished.
                if next.send_closed && next.buffered.is_empty() {
                    next.finished = true;
                }
                BTreeSet::from(["pulse.channel.recv".to_string()])
            }
            SessionAction::Close => {
                next.send_closed = true;
                if next.buffered.is_empty() {
                    next.finished = true;
                }
                BTreeSet::from(["pulse.channel.close".to_string()])
            }
        };
        Ok(StepOutcome { next, effects })
    }
}

/// The networking claim graph.
pub fn claim_graph() -> (AssuranceGraph, tpt_concord_core::ClaimID) {
    use tpt_concord_core::{ClaimID, ObligationID, ScopeID};

    let mut g = AssuranceGraph::new();
    g.add_scope(Scope::new(
        ScopeID::new("scope.network.ordered-channel").unwrap(),
        "Reliable, ordered channels (session types over a TCP-like transport); \
         lossy/unordered transports are out of scope.",
    ))
    .unwrap();

    let claim_id = ClaimID::new("claim.network.session-conformance").unwrap();
    let mut claim = Claim::new(
        claim_id.clone(),
        "Session-typed channels admit exactly their typed message sequences: no send \
         after close, receives match the in-flight message, and close drains before \
         finishing.",
    );
    claim.scopes = vec![ScopeID::new("scope.network.ordered-channel").unwrap()];
    g.add_claim(claim).unwrap();

    for (id, desc) in [
        (
            "obligation.network.session-ordering",
            "Send/recv ordering matches the session type (model-checked)",
        ),
        (
            "obligation.network.close-drains",
            "Close drains buffered messages before the channel finishes",
        ),
        (
            "obligation.network.wire-interop",
            "Wire-level conformance with third-party peers (see keystone module)",
        ),
    ] {
        g.add_obligation(Obligation::new(ObligationID::new(id).unwrap(), desc))
            .unwrap();
        g.require_obligation(&claim_id, &ObligationID::new(id).unwrap())
            .unwrap();
    }

    for (ev_id, ob_id, desc) in [
        (
            "evidence.network.session-model",
            "obligation.network.session-ordering",
            "SessionChannelModel exhaustive check",
        ),
        (
            "evidence.network.close-drains",
            "obligation.network.close-drains",
            "Close/drain lifecycle model check",
        ),
    ] {
        let mut ev = EvidenceRecord::new(
            tpt_concord_core::EvidenceID::new(ev_id).unwrap(),
            EvidenceKind::ModelCheck,
            AssuranceLevel::ModelChecked,
            desc,
        );
        ev.validate();
        g.add_evidence(ev).unwrap();
        g.discharge(
            &ObligationID::new(ob_id).unwrap(),
            &tpt_concord_core::EvidenceID::new(ev_id).unwrap(),
        )
        .unwrap();
    }

    (g, claim_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_concord_model::run;
    use tpt_concord_release::{evaluate, Policy};

    #[test]
    fn typed_session_completes() {
        let m = SessionChannelModel;
        let session = [
            SessionAction::Send("req".into()),
            SessionAction::Recv("req".into()),
            SessionAction::Send("resp".into()),
            SessionAction::Recv("resp".into()),
            SessionAction::Close,
        ];
        let records = run(&m, &session).unwrap();
        assert!(records.last().unwrap().after.finished);
    }

    #[test]
    fn session_type_violation_is_rejected() {
        let m = SessionChannelModel;
        // Send A, then try to receive B: type mismatch, not a silent skip.
        let state = ChannelState {
            buffered: vec!["A".into()],
            send_closed: false,
            finished: false,
        };
        assert!(matches!(
            m.step(&state, &SessionAction::Recv("B".into())),
            Err(SessionError::TypeMismatch { expected, found }) if expected == "B" && found == "A"
        ));
    }

    #[test]
    fn receive_on_empty_would_block() {
        let m = SessionChannelModel;
        assert!(matches!(
            m.step(&m.initial_state(), &SessionAction::Recv("x".into())),
            Err(SessionError::WouldBlock)
        ));
    }

    #[test]
    fn close_drains_before_finishing() {
        let m = SessionChannelModel;
        // Close with a message in flight: not finished, but sends refused.
        let after_close = m
            .step(
                &ChannelState {
                    buffered: vec!["pending".into()],
                    send_closed: false,
                    finished: false,
                },
                &SessionAction::Close,
            )
            .unwrap()
            .next;
        assert!(after_close.send_closed);
        assert!(!after_close.finished);
        assert!(matches!(
            m.step(&after_close, &SessionAction::Send("late".into())),
            Err(SessionError::SendAfterClose)
        ));
        // Drain the buffer: now finished.
        let drained = m
            .step(&after_close, &SessionAction::Recv("pending".into()))
            .unwrap()
            .next;
        assert!(drained.finished);
    }

    #[test]
    fn network_claim_established_except_interop() {
        let (g, claim) = claim_graph();
        assert!(g.validate().is_ok());
        let eval = g.evaluate(&claim).unwrap();
        assert!(!eval.established);
        assert!(eval
            .unmet_obligations
            .iter()
            .any(|u| u.obligation.as_str() == "obligation.network.wire-interop"));

        let mut policy = Policy::new("network-assurance");
        policy.require_claim(claim.clone(), AssuranceLevel::ModelChecked);
        assert!(!evaluate(&g, &policy).is_approved());
    }
}
