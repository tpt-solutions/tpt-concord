// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Scientific & engineering systems assurance infrastructure (spec §12).
//!
//! Grounding: `tpt-solutions/tpt-physics-engine` (rigid body dynamics,
//! collision detection, constraint solving), `tpt-solutions/tpt-system-zero`
//! (orbital mechanics, Kalman filters, Lambert solver), `tpt-solutions/
//! tpt-ignis` (FFT pipelines).
//!
//! These systems are numerically validated, not proven: the claims here are
//! about properties that hold *within explicitly bounded scopes* —
//! determinism of stepping, conservation bounds for the symplectic case,
//! spectral round-trip identity, and estimator consistency. Scope bounds
//! (step size, collision-free regimes) are part of the claims themselves.

use std::collections::BTreeSet;
use tpt_concord_core::{
    AssuranceGraph, AssuranceLevel, Claim, EvidenceKind, EvidenceRecord, Obligation, Scope,
};
use tpt_concord_model::{DeterministicMachine, StepOutcome};

/// A damped harmonic oscillator integrated with fixed steps — the canonical
/// checkable physics core: exact solutions exist, so integration error is
/// measurable and bounded claims are meaningful.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OscillatorState {
    /// Position.
    pub x: f64,
    /// Velocity.
    pub v: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OscillatorParams {
    /// Undamped angular frequency (spring stiffness).
    pub omega: f64,
    /// Damping coefficient.
    pub zeta: f64,
    /// Fixed step size.
    pub dt: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepInput {
    Step,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum OscillatorError {
    #[error("state diverged: |x| = {0} exceeded the model bound")]
    Diverged(f64),
}

/// Semi-implicit (symplectic) Euler stepper: `v += a(x)·dt` then `x += v·dt`.
///
/// Deterministic by construction; conservation claims hold only within the
/// declared step-size scope — and the model *checks* that by refusing to
/// step once the state leaves the bounded regime.
#[derive(Debug, Clone, Copy)]
pub struct SymplecticOscillatorModel(pub OscillatorParams);

impl DeterministicMachine for SymplecticOscillatorModel {
    type State = OscillatorState;
    type Input = StepInput;
    type Error = OscillatorError;

    fn initial_state(&self) -> OscillatorState {
        OscillatorState { x: 1.0, v: 0.0 }
    }

    fn step(
        &self,
        s: &OscillatorState,
        _input: &StepInput,
    ) -> Result<StepOutcome<OscillatorState>, OscillatorError> {
        let p = self.0;
        // Acceleration of a damped oscillator: a = -ω²x - 2ζω v.
        let a = -p.omega * p.omega * s.x - 2.0 * p.zeta * p.omega * s.v;
        let v = s.v + a * p.dt;
        let x = s.x + v * p.dt;
        if !x.is_finite() || x.abs() > 1e6 {
            return Err(OscillatorError::Diverged(x.abs()));
        }
        Ok(StepOutcome {
            next: OscillatorState { x, v },
            effects: BTreeSet::from(["physics.integrate".to_string()]),
        })
    }
}

/// Energy of the oscillator at a state (relative to the unit initial energy
/// scale ω²/2).
pub fn energy(state: &OscillatorState, p: &OscillatorParams) -> f64 {
    0.5 * state.v * state.v + 0.5 * p.omega * p.omega * state.x * state.x
}

/// The science claim graph: every numeric claim is scope-bounded.
pub fn claim_graph() -> (AssuranceGraph, tpt_concord_core::ClaimID) {
    use tpt_concord_core::{ClaimID, ObligationID, ScopeID};

    let mut g = AssuranceGraph::new();
    g.add_scope(Scope::new(
        ScopeID::new("scope.science.bounded-steps").unwrap(),
        "Integration claims hold for fixed step sizes in the validated regime \
         (dt·ω well under 1) and collision-free motion; collision response and \
         adaptive stepping are separate claims.",
    ))
    .unwrap();

    let claim_id = ClaimID::new("claim.science.numerical-conformance").unwrap();
    let mut claim = Claim::new(
        claim_id.clone(),
        "TPT numerical systems step deterministically, conserve energy within a stated \
         bound in the symplectic regime, and round-trip through their transforms.",
    );
    claim.scopes = vec![ScopeID::new("scope.science.bounded-steps").unwrap()];
    g.add_claim(claim).unwrap();

    for (id, desc) in [
        (
            "obligation.science.stepping-determinism",
            "Same state + params + step count ⇒ bit-identical state (model-checked)",
        ),
        (
            "obligation.science.energy-bound",
            "Symplectic energy drift stays within the declared bound over the sweep",
        ),
        (
            "obligation.science.spectral-roundtrip",
            "FFT⁻¹(FFT(x)) ≈ x within tolerance (property-tested upstream)",
        ),
        (
            "obligation.science.estimator-consistency",
            "Kalman estimator error contracts within bounds (system-zero)",
        ),
    ] {
        g.add_obligation(Obligation::new(ObligationID::new(id).unwrap(), desc))
            .unwrap();
        g.require_obligation(&claim_id, &ObligationID::new(id).unwrap())
            .unwrap();
    }

    for (ev_id, ob_id, kind, level, desc) in [
        (
            "evidence.science.determinism",
            "obligation.science.stepping-determinism",
            EvidenceKind::ModelCheck,
            AssuranceLevel::ModelChecked,
            "SymplecticOscillatorModel determinism check",
        ),
        (
            "evidence.science.energy",
            "obligation.science.energy-bound",
            EvidenceKind::Simulation,
            AssuranceLevel::ModelChecked,
            "Bounded energy-drift sweep",
        ),
        (
            "evidence.science.roundtrip",
            "obligation.science.spectral-roundtrip",
            EvidenceKind::PropertyTestCampaign,
            AssuranceLevel::PropertyTested,
            "FFT round-trip property campaign (tpt-ignis)",
        ),
    ] {
        let mut ev = EvidenceRecord::new(
            tpt_concord_core::EvidenceID::new(ev_id).unwrap(),
            kind,
            level,
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

    fn params() -> OscillatorParams {
        OscillatorParams {
            omega: 1.0,
            zeta: 0.05,
            dt: 0.01,
        }
    }

    #[test]
    fn stepping_is_deterministic() {
        let m = SymplecticOscillatorModel(params());
        let a = run(&m, &[StepInput::Step; 100]).unwrap();
        let b = run(&m, &[StepInput::Step; 100]).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn symplectic_energy_drift_stays_bounded() {
        // The conservation claim is about the *undamped* symplectic regime;
        // damping is a separate (decaying-energy) behaviour tested below.
        let p = OscillatorParams {
            omega: 1.0,
            zeta: 0.0,
            dt: 0.01,
        };
        let m = SymplecticOscillatorModel(p);
        let records = run(&m, &[StepInput::Step; 1000]).unwrap();
        let e0 = energy(&OscillatorState { x: 1.0, v: 0.0 }, &p);
        for r in &records {
            let e = energy(&r.after, &p);
            let drift = (e - e0).abs() / e0;
            assert!(
                drift < 0.01,
                "energy drift {} exceeded 1% at step {}",
                drift,
                r.step
            );
        }
    }

    #[test]
    fn damped_system_decays() {
        let p = params();
        let m = SymplecticOscillatorModel(p);
        let records = run(&m, &[StepInput::Step; 2000]).unwrap();
        let late = &records[records.len() - 1].after;
        assert!(
            late.x.abs() < 0.5,
            "damped oscillator should decay; x = {}",
            late.x
        );
    }

    #[test]
    fn divergence_is_an_error_not_a_silent_number() {
        // An unstable regime (huge dt with high omega) must error, not NaN.
        let wild = OscillatorParams {
            omega: 10.0,
            zeta: -5.0, // negative damping: exponentially growing
            dt: 0.5,
        };
        let m = SymplecticOscillatorModel(wild);
        let mut state = m.initial_state();
        let mut diverged = false;
        for _ in 0..200 {
            match m.step(&state, &StepInput::Step) {
                Ok(outcome) => state = outcome.next,
                Err(OscillatorError::Diverged(_)) => {
                    diverged = true;
                    break;
                }
            }
        }
        assert!(diverged, "unstable regime must be caught by the bound");
    }

    #[test]
    fn science_claim_open_on_estimator_consistency() {
        let (g, claim) = claim_graph();
        assert!(g.validate().is_ok());
        let eval = g.evaluate(&claim).unwrap();
        // system-zero Kalman consistency: not dischargeable yet.
        assert!(!eval.established);
        assert!(eval
            .unmet_obligations
            .iter()
            .any(|u| u.obligation.as_str() == "obligation.science.estimator-consistency"));

        let mut policy = Policy::new("science-assurance");
        policy.require_claim(claim.clone(), AssuranceLevel::ModelChecked);
        assert!(!evaluate(&g, &policy).is_approved());
    }
}
