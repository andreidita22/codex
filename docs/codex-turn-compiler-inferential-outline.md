# Codex Turn Compiler: Inferential Outline

This document is a compact formalization companion to:
- [codex-turn-compiler-full.md](/home/rose/work/codex/fork/docs/codex-turn-compiler-full.md)

It translates the long-form conceptual prose into a concise pseudo-formal structure:
- definitions of core entities
- invariants and laws
- stage contracts
- main claims and what each claim entails

## 1. Core Objective

Replace transcript-centric continuity with typed state continuity for turns.

Formally:
- Old continuity substrate: `C_old = compact(transcript)`
- Target continuity substrate: `C_new = reduce_typed(artifacts, effects, residuals)`

Goal:
- Minimize semantic drift and authority laundering while preserving operational continuity.

## 2. Primitive Domains

Let:
- `U` = user surface utterances
- `T` = closed input-type registry
- `O` = operator family registry
- `S` = persistent ODEU state
- `R` = runtime/governance environment
- `A` = emitted typed artifacts
- `Efx` = observed effects

ODEU state shape (conceptual):
- `S = <Q_O, Q_E, Q_D, Q_U, Rr, J, Xi, N>`
- `Q_O`: observed facts/witness refs
- `Q_E`: execution/posture state
- `Q_D`: deontic obligations/constraints
- `Q_U`: unresolved ambiguity/underdetermination
- `Rr`: retained residual witness register
- `J`: rejection/block log
- `Xi`: escalation queue
- `N`: reproposal demands

## 3. Compiler View of a Turn

Define turn compilation:
- `CompileTurn : (u in U, S_prev, T, O, R) -> (A_turn, S_delta, S_next, prune_manifest)`

with:
- `S_next = Refresh(S_prev, S_delta)`
- `prune_manifest` explicitly records promoted vs discarded data.

Key requirement:
- No undeclared implicit carry between stages.

## 4. Stage Graph (Typed Contracts)

Stages:
1. Surface normalization
2. Input-type proposal
3. Input-type adjudication
4. Operator family resolution (conditional)
5. Parameter binding
6. Parameter expansion
7. Execution-packet construction
8. Governance/effect lowering
9. Turn refresh reduction

For each stage `i`:
- input domain `D_i` is closed and explicit
- output schema `Sigma_i` is explicit
- advancement predicate `Adv_i` is explicit
- carry allowlist `Carry_i` is explicit

Constraint:
- boundary crossing is only via `Sigma_i` fields (no hidden residue).

## 5. Semantic Ingress Definitions

Artifacts:
- `semantic_input_surface_record@1`
- `semantic_input_type_candidate_packet@1`
- `semantic_input_type_adjudication@1`

Ingress property:
- `Adjudication.type in T`
- if no confident winner, output one of:
  - bounded ambiguity
  - blocked
  - explicit fallback lane

Never:
- silently open new type outside `T`.

## 6. Operator Compilation Definitions

Artifacts:
- `workflow_operator_candidate_packet@1`
- `workflow_operator_binding@1`
- `workflow_operator_expansion_frame@1`
- `workflow_execution_packet@1`

Operator law:
- no latent operator invention outside registry `O`
- required slots must be:
  - bound, or
  - inherited under declared law, or
  - defaulted under declared law, or
  - marked unresolved

No silent slot fill.

## 7. Refresh/Carry Definitions

Artifacts:
- `odeu_turn_delta@1`
- `odeu_state_snapshot@1`
- `residual_witness_packet@1`
- `carry_prune_manifest@1`

Promotion law:
- field promotable to persistent state iff:
  - emitted in declared artifact schema
  - has provenance metadata
  - has authority posture
  - has dependence refs

Retention law:
- keep only semantically live state, required witnesses, unresolved obligations.

Pruning law:
- discard local non-promoted residue; list reasons in prune manifest.

## 8. Anti-Laundering Laws (Compact Form)

L1. Closed-world ingress:
- `type(u) in T` or explicit unmatched lane.

L2. Boundary emission:
- only schema-emitted fields cross stage boundaries.

L3. Provenance-before-promotion:
- no promotion without provenance + authority posture + dependence refs.

L4. Advisory non-promotion:
- candidate/projection/hypothesis != persistent state by default.

L5. Negative-state preservation:
- blocked/insufficient/authority-missing/not-evaluable states cannot be collapsed into success continuity.

L6. Transcript non-authority:
- transcript is context input, not ambient authority.

L7. No hidden bridge state:
- no undeclared carry caches.

L8. No authority minting by projection:
- expansion/projection can constrain; cannot mint authority.

L9. Re-entry law:
- discarded residue may re-enter only as explicit source/artifact in a later turn.

## 9. Advancement Policy Library (Operational)

Policies (examples):
- `exclusive_winner_above_threshold`
- `exclusive_winner_with_margin`
- `bounded_multi_hypothesis_carry`
- `block_until_required_slots_bound`
- `defaultable_slots_only`
- `human_review_required`
- `fallback_to_freeform_lane`
- `authorization_gate_required`

Interpretation:
- these are not prose hints; they are gate predicates controlling stage advance.

## 10. Principal Claims and Entailments

### Claim C1
`Typed ingress + closed registry reduces semantic drift.`

Entails:
- explicit mismatch handling instead of hidden reinterpretation
- deterministic reclassification behavior under same inputs
- auditable ambiguity instead of silent coercion

### Claim C2
`Operator schema as semantic object is safer than prompt templates.`

Entails:
- explicit slot law and defaults
- explicit pre/post conditions
- explicit sequencing and review/commit posture

### Claim C3
`Stage-gated compilation with explicit carry law prevents assumption smuggling.`

Entails:
- no hidden intermediate reasoning promoted as state
- localized failure diagnostics by stage
- reproducible transition traces

### Claim C4
`Refresh reducer yields state continuity superior to transcript compaction for long arcs.`

Entails:
- continuity preserved by typed deltas, not prose recap
- lower risk of authority drift from summary compression
- explicit prune ledger for what was not carried

### Claim C5
`Separation of operator semantics from effect governance avoids implicit authority widening.`

Entails:
- compile intent without auto-authorizing effect
- preserve downstream governed checks as authority boundary
- avoid write/commit conflation

## 11. Minimal Viable Implementation (Shadow-First)

MVP artifacts:
1. `semantic_input_type_adjudication@1`
2. `workflow_operator_binding@1` (single operator family)
3. `odeu_turn_delta@1`
4. `carry_prune_manifest@1`

MVP mode:
- shadow (diagnostic/emissive), not hard enforcement.

Success criterion:
- same turn input + same prior state + same registries => same typed adjudication/binding/delta output.

## 12. Mapping to Current Fork (Practical)

Current fork already has nearby scaffolding:
- governance packet/projection layer:
  - [core/src/governance/prompt_layers.rs](/home/rose/work/codex/fork/codex-rs/core/src/governance/prompt_layers.rs)
- transition legality/diagnostics taxonomy:
  - [core/src/governance/transitions.rs](/home/rose/work/codex/fork/codex-rs/core/src/governance/transitions.rs)
- compaction-time state artifact:
  - [core/src/governance/thread_memory.rs](/home/rose/work/codex/fork/codex-rs/core/src/governance/thread_memory.rs)
- baton handoff artifact:
  - [core/src/continuation_bridge.rs](/home/rose/work/codex/fork/codex-rs/core/src/continuation_bridge.rs)

Next architectural insertion point:
- add dedicated turn-compiler family/module emitting ingress/operator/delta artifacts
- keep effect governance downstream in existing runtime governance lanes
- keep remote compaction endpoint untouched; wrap around it with typed artifacts

## 13. Bottom Line

The long-form thesis can be expressed as:

- Treat each turn as a compiled object.
- Restrict inference by closed typed domains at each stage.
- Promote only provenance-bearing, authority-typed outputs.
- Persist continuity as ODEU deltas plus residual witnesses.
- Prune non-promoted residue explicitly.

This is the shortest path from conceptual ADEU posture to an implementable, auditable turn architecture.
