# The AI Acceptance Pipeline (spec §7)

AI is an **untrusted proposer**. It may generate implementations, tests,
specifications, proof scripts, model refinements and candidate invariants —
and none of those are trusted merely because an AI generated them. This
document is the wiring guide for the acceptance path every AI-generated
artifact must walk, and the rules that keep evidence types from being
mischaracterized.

## The pipeline

Each stage is enforced by a specific Concord capability. A proposal advances
only by passing the stage; a failure records the rejection point and *nothing
past that point is claimed* (see `tpt_concord_release::AcceptanceTrace`).

| # | Stage | Enforced by | What "pass" means |
|---|-------|-------------|-------------------|
| 1 | AI proposal | — | The artifact exists. Trusted for nothing. |
| 2 | Compiler / type system | `cargo check`, `clippy -D warnings` | The artifact is well-formed Rust. |
| 3 | Specification obligations | `tpt-concord-core` (`AssuranceGraph`) | Every specification property the artifact touches is registered as a Claim with explicit Obligations, scopes and assumptions. |
| 4 | Formal proof / model checking | `tpt-concord-proof` adapters, `tpt-concord-sim` exploration | Each proof obligation is discharged by an artifact whose hash and re-check command are recorded. `sorry`/`admit`/unverifiable output is `Inconclusive`, never proven. |
| 5 | Implementation conformance | `tpt-concord-check` | The implementation's canonical trace matches the executable reference model; deviations are classified and become Counterexamples. |
| 6 | Property testing | `tpt-concord-property` | Spec-derived properties survive a seeded campaign; failures are shrunk to minimal reproducers. |
| 7 | Fuzzing | `tpt-concord-fuzz` | State-transition-aware fuzzing over the model finds no violations; failures land in the corpus with seeds. |
| 8 | Evidence validation | `tpt-concord-evidence` | Every artifact re-verifies: hashes match, traces replay to identical digests, bundles are intact. Evidence the checker cannot re-verify stays `Unvalidated`. |
| 9 | Release gate | `tpt-concord-release` | Policy is satisfied: required claims established at required assurance levels, required obligations discharged, assumptions either allowed or absent. |

The canonical object walking this path is `AcceptanceTrace`
(`tpt-concord-release`): a serializable record of stage outcomes. A rejected
proposal's trace ends at its failure stage — a consumer can see at a glance
that stages 5–9 were never reached, and no output may claim them.

## Assurance-level distinctions (spec §6, §8)

Concord exists partly so this section never becomes negotiable:

- **`fully_proven` / `partially_proven`** mean formal proof artifacts exist
  and independently re-verify. Nothing else reaches these levels — not
  exhaustive exploration, not a million fuzz executions.
- **`model_checked`** means systematic exploration of a (finite) model. It is
  strong evidence *about the model*, transferred to the implementation only
  through conformance (stage 5).
- **`property_tested` / `fuzz_tested`** mean sampling found no violations.
  Absence of evidence is not evidence of absence; campaigns are recorded with
  their seeds so they are reproducible, but a clean campaign is still a
  sampling result.
- **`differentially_checked`** means two systems agreed on observed traces.
  Agreement of two implementations is not correctness; it is consistency.
- **`type_checked`** is static structural knowledge. **`specified`** is a
  statement of intent.

On the §6 scale, a gate may accept "at least model_checked" for a claim; the
`strictness_rank` is a policy hint, **not an equivalence**. Where the exact
kind of evidence matters, policy must name the evidence kind
(`Policy::required_obligations` + obligation→evidence links preserve kind
exactly: a `FuzzCampaign` artifact can never discharge an obligation whose
evidence slot is typed `LeanProof` without the mismatch being visible in the
graph).

The one-sentence version: **a fuzz campaign is never a mathematical proof,
and no tool, gate or document in Concord may imply otherwise.**
