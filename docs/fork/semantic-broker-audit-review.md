# Semantic Broker Audit Review

This note reviews the new semantic-broker design inputs:

- [gptpro-audit-07-semantic-broker-initial.md](/home/rose/work/codex/fork/docs/fork/gptpro-audit-07-semantic-broker-initial.md)
- [gptpro-audit-08-semantic-broker-followup.md](/home/rose/work/codex/fork/docs/fork/gptpro-audit-08-semantic-broker-followup.md)
- [semantic-broker-followup-assessment.md](/home/rose/work/codex/fork/docs/fork/semantic-broker-followup-assessment.md)

## Judgment

This is a real next custom-module candidate.

The strongest accepted conclusion is:

- the broker should be a dedicated prompt-ingress semantic module with a prompt-only active packet in `v0`

The most important correction between the first and second drafts is also accepted:

- the broker should not be designed as a dead-end deterministic verb matcher
- it should be designed from day 1 as bounded semantic adjudication over a typed schema registry

That does not mean `v0` should become large. It means the crate API should preserve the right long-term ontology while the first implementation stays deterministic.

For implementation naming, the narrow deterministic first slice should be treated as:

- semantic broker `v0`

That leaves future model-assisted selection as `v1` or later.

## Accepted Findings

### 1. The right seam is prompt ingress, not durable history

The broker belongs near the prompt-construction path, not inside compaction, thread memory, continuation bridge, or history normalization.

The clean insertion seam in the current fork is:

- [turn.rs](/home/rose/work/codex/fork/codex-rs/core/src/session/turn.rs)

Specifically:

- after tool availability and base instructions are known
- before `build_prompt(...)`

That is the right place to attach a prompt-only overlay without turning durable history into a second broker law surface.

### 2. The broker should be separate from governance layering

The current fork already has governance packet and prompt-layering infrastructure in:

- [packets.rs](/home/rose/work/codex/fork/codex-rs/core/src/governance/packets.rs)
- [compiler.rs](/home/rose/work/codex/fork/codex-rs/core/src/governance/compiler.rs)
- [prompt_layers.rs](/home/rose/work/codex/fork/codex-rs/core/src/governance/prompt_layers.rs)

That is adjacent to the broker, but it is not the broker.

Governance layering expresses:

- precedence and authority relationships among already-known prompt sections

The broker should express:

- turn-specific semantic resolution over typed schema candidates
- an active prompt overlay for the current turn

Those should remain distinct.

### 3. The second draft fixes the main long-term architecture risk

The second draft and the updated assessment correctly introduce the missing long-term concepts:

- canonical operator meta-schema
- workflow/procedure schema instances
- alias and synonym tables
- runtime registry snapshot
- candidate narrowing
- bounded adjudication
- typed resolution
- active packet construction

That is the right architecture.

The main improvement is that `v0` no longer risks fossilizing into:

- a hard-coded list of verbs
- a special-case prompt router

Instead, the deterministic `v0` resolver can be the first implementation of a broader bounded-selection pipeline.

### 4. The packet should stay typed and small

The packet should be primarily:

- refs
- bindings
- confidence
- events

not:

- a second giant semantic prompt document

The first draft was directionally right about prompt-only packet injection. The second draft improves the internal structure. The packet should carry schema/operator/track references and typed bindings first, with prose summary as secondary support.

### 5. Interactive schema authoring is real, but not `v0` scope

The second draft is also right to treat interactive schema authoring as bounded structured workflow, not as free-form markdown authoring.

That matters for future design because it reinforces:

- meta-schema ownership
- schema lifecycle
- alias-vs-new-schema distinction
- guided clarification

But this should remain future-facing for now. It should inform crate design, not expand the initial implementation slice.

## Required Narrowing

### 1. No generic fork hub

This should not become a generic broker or meta-module that mediates every fork feature.

The correct shape remains:

- a domain-specific semantic crate
- a thin runtime adapter in `codex-core`
- small insertion hooks in hot upstream files

### 2. No durable artifact semantics in `v0`

The broker should not produce:

- thread-memory artifacts
- continuation-bridge artifacts
- prompt/history mutations that compaction can ingest as conversation content

`v0` should be:

- prompt-only overlay
- current-turn active packet

### 3. No protocol expansion in `v0`

Do not add broker semantics to protocol models in the first slice.

In particular:

- do not extend `DeveloperInstructions`
- do not create a protocol-level packet variant yet

The current fork can render the active packet as a tagged prompt overlay owned by the broker module itself.

### 4. Registry precedence must be explicit

The updated assessment correctly calls out one missing piece:

- registry source precedence and conflict resolution law

That must be explicit in the implementation spec, even if the first registry only includes builtin schemas.

The design must be able to answer:

- what wins when builtin, project, user, skill, or governance sources collide
- how conflict is surfaced
- whether precedence is silent or requires an explicit conflict event

For `v0`, the preferred rule is fail-closed:

- builtin-only registry
- duplicate schema/operator/alias conflicts reject the snapshot
- no silent precedence override

## Recommended Crate Shape

New workspace crate:

- `codex-rs/semantic-broker`

Cargo package:

- `codex-semantic-broker`

Runtime adapter in core:

- `core/src/semantic_broker_runtime.rs`

Current insertion seam:

- [turn.rs](/home/rose/work/codex/fork/codex-rs/core/src/session/turn.rs)

Likely initial owned modules:

- `registry.rs`
- `candidates.rs`
- `adjudicator.rs`
- `packet.rs`
- `render.rs`

## Decisions Carried Forward

These decisions should guide the new family:

- broker law should live in a dedicated crate
- `codex-core` should remain runtime/adapter wiring
- the seam is prompt ingress, not compaction/history
- `v0` should be prompt-only and deterministic
- the API should already be registry/candidate/adjudication shaped
- source precedence and conflict handling must be explicit
- `v0` should be feature-gated and default off
- interactive schema authoring is future-adjacent, not initial scope
- do not build a generic fork hub around this work

## Recommended Next Family

The next family should be:

1. lock the current prompt-ingress seam and governance-adjacent assumptions
2. scaffold `codex-semantic-broker` with typed registry/candidate/resolution/packet types
3. wire a deterministic broker runtime into `run_sampling_request(...)`
4. single-source active-packet rendering/tag/schema ownership from the broker crate
5. narrow exports and clean up test doctrine after the seam settles
