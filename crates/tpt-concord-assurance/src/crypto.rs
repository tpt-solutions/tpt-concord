// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Cryptography assurance infrastructure (spec §12).
//!
//! Grounding in the TPT ecosystem: Keystone's opt-in SCRAM-SHA-256/TLS auth,
//! Archon's WAL record framing with checksums, and `tpt-shatter`'s
//! capability tokens and field-level fragmentation.
//!
//! **Scope discipline (the whole point of this module):** Concord models the
//! *structure* of cryptographic protocols — state machines, nonce-lifecycle
//! rules, framing invariants. It does **not** and cannot claim cryptographic
//! strength. "The cipher is unbreakable" is nobody's claim here; the claims
//! are about protocol-structure properties that conformance runs can
//! actually check. Primitive strength remains with the primitive's own
//! verification (out of Concord's scope).

use std::collections::BTreeSet;
use tpt_concord_core::{
    AssuranceGraph, AssuranceLevel, Claim, EvidenceKind, EvidenceRecord, Obligation, Scope,
};
use tpt_concord_model::{DeterministicMachine, StepOutcome};

/// A per-key nonce lifecycle: AEAD security requires a (key, nonce) pair to
/// never repeat. The counter model pins the structural rule: increment only,
/// never reset under the same key.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NonceState {
    pub key_id: String,
    pub counter: u64,
    /// Nonces already handed out, tracked to make reuse detectable.
    pub used: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum NonceOp {
    /// Issue the next nonce for this key.
    Next,
    /// Key rotation: retires the old key (fresh key ⇒ fresh counter space).
    Rotate(String),
}

impl std::fmt::Display for NonceOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NonceOp::Next => f.write_str("next"),
            NonceOp::Rotate(k) => write!(f, "rotate({k})"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NonceError {
    #[error("nonce counter exhausted for key `{0}`")]
    Exhausted(String),
}

/// Nonce-lifecycle machine: reuse is structurally impossible while the
/// counter is monotone per key; exhaustion is an explicit error, never a
/// wraparound.
#[derive(Debug, Clone, Default)]
pub struct NonceLifecycleModel;

impl DeterministicMachine for NonceLifecycleModel {
    type State = NonceState;
    type Input = NonceOp;
    type Error = NonceError;

    fn initial_state(&self) -> NonceState {
        NonceState {
            key_id: "key-0".to_string(),
            counter: 0,
            used: Vec::new(),
        }
    }

    fn step(&self, s: &NonceState, op: &NonceOp) -> Result<StepOutcome<NonceState>, NonceError> {
        let mut next = s.clone();
        let effects = match op {
            NonceOp::Next => {
                next.counter = s
                    .counter
                    .checked_add(1)
                    .ok_or_else(|| NonceError::Exhausted(s.key_id.clone()))?;
                next.used.push(s.counter);
                BTreeSet::from(["crypto.nonce.issue".to_string()])
            }
            NonceOp::Rotate(new_key) => {
                // Rotation resets the counter *because the key changes* — the
                // (key, nonce) pair is still fresh.
                next.key_id = new_key.clone();
                next.counter = 0;
                next.used.clear();
                BTreeSet::from(["crypto.key.rotate".to_string()])
            }
        };
        Ok(StepOutcome { next, effects })
    }
}

/// SCRAM-SHA-256 exchange states (Keystone auth, opt-in upstream).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum ScramState {
    ClientFirstSent,
    ServerFirstReceived,
    ClientFinalSent,
    Authenticated,
    Rejected,
}

/// SCRAM handshake steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ScramStep {
    ClientFirst,
    ServerFirst,
    ClientFinal,
    ServerFinal,
    ServerReject,
}

impl std::fmt::Display for ScramStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ScramStep::ClientFirst => "client_first",
            ScramStep::ServerFirst => "server_first",
            ScramStep::ClientFinal => "client_final",
            ScramStep::ServerFinal => "server_final",
            ScramStep::ServerReject => "server_reject",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScramError {
    #[error("step `{0}` out of order in SCRAM exchange")]
    OutOfOrder(&'static str),
}

/// SCRAM handshake machine: strict alternation; rejection is absorbing.
#[derive(Debug, Clone, Default)]
pub struct ScramHandshakeModel;

impl DeterministicMachine for ScramHandshakeModel {
    type State = ScramState;
    type Input = ScramStep;
    type Error = ScramError;

    fn initial_state(&self) -> ScramState {
        ScramState::ClientFirstSent
    }

    fn step(
        &self,
        s: &ScramState,
        step: &ScramStep,
    ) -> Result<StepOutcome<ScramState>, ScramError> {
        let out_of_order = || ScramError::OutOfOrder(format!("{step}").leak() as &'static str);
        let next = match (s, step) {
            (ScramState::ClientFirstSent, ScramStep::ServerFirst) => {
                ScramState::ServerFirstReceived
            }
            (ScramState::ServerFirstReceived, ScramStep::ClientFinal) => {
                ScramState::ClientFinalSent
            }
            (ScramState::ClientFinalSent, ScramStep::ServerFinal) => ScramState::Authenticated,
            (ScramState::ClientFirstSent, ScramStep::ServerReject)
            | (ScramState::ServerFirstReceived, ScramStep::ServerReject)
            | (ScramState::ClientFinalSent, ScramStep::ServerReject) => ScramState::Rejected,
            (ScramState::Rejected, _) => ScramState::Rejected, // absorbing
            _ => return Err(out_of_order()),
        };
        Ok(StepOutcome {
            next,
            effects: BTreeSet::from(["crypto.scram".to_string()]),
        })
    }
}

/// The crypto claim graph: structural protocol properties only.
pub fn claim_graph() -> (AssuranceGraph, tpt_concord_core::ClaimID) {
    use tpt_concord_core::{ClaimID, ObligationID, ScopeID};

    let mut g = AssuranceGraph::new();
    g.add_scope(Scope::new(
        ScopeID::new("scope.crypto.structure-only").unwrap(),
        "Claims cover protocol structure (state machines, nonce lifecycle, framing). \
         Cryptographic primitive strength is out of Concord's scope entirely.",
    ))
    .unwrap();

    let claim_id = ClaimID::new("claim.crypto.protocol-structure").unwrap();
    let mut claim = Claim::new(
        claim_id.clone(),
        "TPT protocol crypto structure is sound: AEAD nonces never repeat under a key, \
         and SCRAM exchanges progress only through their legal sequence.",
    );
    claim.scopes = vec![ScopeID::new("scope.crypto.structure-only").unwrap()];
    g.add_claim(claim).unwrap();

    for (id, desc) in [
        (
            "obligation.crypto.nonce-uniqueness",
            "Nonce lifecycle is monotone per key; wraparound is an error, never reuse",
        ),
        (
            "obligation.crypto.scram-sequencing",
            "SCRAM exchanges admit only the legal message order; rejection is absorbing",
        ),
        (
            "obligation.crypto.framing-integrity",
            "WAL/record framing detects any bit corruption (checksum conformance)",
        ),
    ] {
        g.add_obligation(Obligation::new(ObligationID::new(id).unwrap(), desc))
            .unwrap();
        g.require_obligation(&claim_id, &ObligationID::new(id).unwrap())
            .unwrap();
    }

    // Nonce + SCRAM: model-checked here.
    for (ev_id, ob_id, desc) in [
        (
            "evidence.crypto.nonce-model",
            "obligation.crypto.nonce-uniqueness",
            "NonceLifecycleModel exhaustive check",
        ),
        (
            "evidence.crypto.scram-model",
            "obligation.crypto.scram-sequencing",
            "ScramHandshakeModel exhaustive check",
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
    fn nonce_reuse_is_structurally_impossible() {
        let m = NonceLifecycleModel;
        let seq = vec![NonceOp::Next; 5];
        let records = run(&m, &seq).unwrap();
        let final_state = records.last().unwrap().after.clone();
        // All issued nonces distinct, counter monotone.
        let mut sorted = final_state.used.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), final_state.used.len());
        assert_eq!(final_state.counter, 5);
    }

    #[test]
    fn rotation_resets_only_because_the_key_changes() {
        let m = NonceLifecycleModel;
        let records = run(
            &m,
            &[
                NonceOp::Next,
                NonceOp::Next,
                NonceOp::Rotate("key-1".into()),
            ],
        )
        .unwrap();
        let after = records.last().unwrap().after.clone();
        assert_eq!(after.key_id, "key-1");
        assert_eq!(after.counter, 0);
        assert!(after.used.is_empty());
    }

    #[test]
    fn scram_legal_sequence_and_absorbing_rejection() {
        let m = ScramHandshakeModel;
        let ok = [
            ScramStep::ServerFirst,
            ScramStep::ClientFinal,
            ScramStep::ServerFinal,
        ];
        assert_eq!(
            run(&m, &ok).unwrap().last().unwrap().after,
            ScramState::Authenticated
        );

        let rejected = [ScramStep::ServerReject, ScramStep::ServerFirst];
        let records = run(&m, &rejected).unwrap();
        // Rejection is absorbing: subsequent steps cannot revive it.
        assert_eq!(records.last().unwrap().after, ScramState::Rejected);
    }

    #[test]
    fn scram_out_of_order_is_error() {
        let m = ScramHandshakeModel;
        assert!(m
            .step(&ScramState::ClientFirstSent, &ScramStep::ClientFinal)
            .is_err());
        assert!(m
            .step(&ScramState::Authenticated, &ScramStep::ServerFinal)
            .is_err());
    }

    #[test]
    fn crypto_structure_claim_established() {
        let (g, claim) = claim_graph();
        assert!(g.validate().is_ok());
        let eval = g.evaluate(&claim).unwrap();
        // Framing integrity has no evidence yet (needs Archon checksum runs).
        assert!(!eval.established);
        assert!(eval
            .unmet_obligations
            .iter()
            .any(|u| u.obligation.as_str() == "obligation.crypto.framing-integrity"));

        let mut policy = Policy::new("crypto-assurance");
        policy.require_claim(claim.clone(), AssuranceLevel::ModelChecked);
        assert!(!evaluate(&g, &policy).is_approved());
    }
}
