# Semantic Broker Extraction Plan

This plan takes the accepted conclusions from [semantic-broker-audit-review.md](/home/rose/work/codex/fork/docs/fork/semantic-broker-audit-review.md) and turns them into a concrete extraction program.

This is the next custom-module family after:

- context-maintenance
- agent-observability

## Scope

In scope:

- prompt-ingress semantic resolution for the active turn
- typed schema registry ownership
- deterministic candidate narrowing and adjudication
- prompt-only active packet construction and rendering
- explicit runtime adapter seam in `codex-core`

Out of scope:

- durable history mutation
- context-maintenance artifact integration
- thread memory / continuation bridge semantics
- protocol-level packet variants
- model-based adjudication
- interactive schema authoring implementation
- generic fork hub design

## Goal

Move fork-owned semantic prompt-ingress law out of `codex-core` and into a dedicated crate, while keeping prompt assembly, session runtime, and tool orchestration in their current crates.

The desired end state is:

- `codex-semantic-broker` owns registry, candidate, adjudication, and packet law
- `codex-core` acts as runtime adapter and prompt-injection site
- the broker packet is prompt-only in `v0` and does not become durable conversation content

## Versioning And Rollout

This family is the narrow deterministic first slice:

- semantic broker `v0`

Recommended rollout shape:

- feature key: `semantic_broker`
- default: off
- packet schema family rooted at `codex.semantic_broker.active_packet.v0`

## Real Semantic Boundary

The semantic owner should control:

- schema registry snapshot structure
- registry source precedence and conflict handling
- candidate narrowing
- deterministic schema/operator adjudication
- confidence and separation semantics
- lexical evolution / clarify event typing
- active packet schema
- active packet rendering contract

The semantic owner should not control:

- history normalization
- context-maintenance route/disposition logic
- durable artifact retention
- session lifecycle orchestration
- tool execution
- build-prompt orchestration itself

The runtime side should still own:

- collecting current prompt candidates and turn context
- invoking the broker with runtime inputs
- injecting the rendered packet into prompt construction
- keeping the packet prompt-only in `v0`

## Current Touch Points

Primary current ingress path:

- [turn.rs](/home/rose/work/codex/fork/codex-rs/core/src/session/turn.rs)

Current initial-context assembly that should remain separate:

- [mod.rs](/home/rose/work/codex/fork/codex-rs/core/src/session/mod.rs)

Adjacent governance layering that should remain distinct:

- [packets.rs](/home/rose/work/codex/fork/codex-rs/core/src/governance/packets.rs)
- [compiler.rs](/home/rose/work/codex/fork/codex-rs/core/src/governance/compiler.rs)
- [prompt_layers.rs](/home/rose/work/codex/fork/codex-rs/core/src/governance/prompt_layers.rs)

Prompt/history shaping that should not become broker-owned:

- [history.rs](/home/rose/work/codex/fork/codex-rs/core/src/context_manager/history.rs)
- [context_maintenance.rs](/home/rose/work/codex/fork/codex-rs/core/src/context_maintenance.rs)
- [compact.rs](/home/rose/work/codex/fork/codex-rs/core/src/compact.rs)

## Desired Crate Shape

New workspace crate:

- `codex-rs/semantic-broker`

Cargo package:

- `codex-semantic-broker`

Expected direct dependencies:

- `codex-protocol`
- `serde`
- `serde_json`
- optionally `thiserror`

Dependencies it should not take:

- `codex-core`
- `codex-tools`
- TUI/app-server crates
- context-maintenance crates

Likely owned modules:

- `registry.rs`
- `candidates.rs`
- `adjudicator.rs`
- `packet.rs`
- `render.rs`

Expected adapter file in core:

- `core/src/semantic_broker_runtime.rs`

## Target Runtime Shape

After extraction:

- `Session` / `TurnContext` remain runtime-side
- `run_sampling_request(...)` asks the broker runtime for an active packet immediately before `build_prompt(...)`
- the runtime adapter gathers current broker input from:
  - current turn input
  - current tool surface
  - current track/session hints
  - builtin/project schema availability
- the runtime adapter renders the broker-owned packet into prompt input
- the packet is not stored as durable conversation history in v1

## Day-1 Data Model Direction

The crate API should already look like bounded semantic selection, even with a deterministic `v0` adjudicator.

That means day-1 types should include the shape of:

- `SchemaRegistrySnapshot`
- `CandidateSet`
- `SchemaAdjudicator`
- `BrokerResolution`
- `ResolutionConfidence`
- `ActiveContextPacket`
- `LexicalEvolutionEvent`

`v0` adjudication can still be deterministic, but the API should not be shaped like:

- one hard-coded verb matcher returning prose

## Registry Precedence Requirement

The extraction must define typed precedence/conflict behavior for registry sources.

Likely future source families:

- builtin
- project
- user-approved
- skill-provided
- governance-provided

`v0` may start with builtin-only schemas, but the precedence law must still exist.

It should answer:

- source ordering
- collision handling
- conflict visibility
- whether collision yields deterministic winner, rejection, or conflict event

`v0` should be fail-closed:

- only builtin schemas are loaded
- duplicate schema ID with different content rejects the registry snapshot
- duplicate operator ID with incompatible definition rejects the registry snapshot
- alias target conflicts reject the registry snapshot
- exact duplicate content may be ignored only if explicitly tested

`v0` should not implement silent precedence override.

## PR Sequence

### PR S0 — Prompt-Ingress Behavior Locks

Purpose:

- freeze the current ingress seam before broker ownership moves in

Lock:

- current `build_initial_context(...)` behavior remains separate from turn-active overlay logic
- governance prompt-layering remains separate from any future broker packet
- `run_sampling_request(...)` remains the intended live seam
- no current context-maintenance tests regress

Testing note:

- S0 should stay behavioral and seam-oriented
- it should not add production broker overlay machinery before the broker exists

Preferred test locations:

- `codex-core` prompt/session tests
- small governance-adjacent ordering tests

### PR S1 — Create `codex-semantic-broker` And Move Type Ownership

Purpose:

- introduce the new crate and give it typed ownership of registry/candidate/resolution/packet law

Move or create:

- registry snapshot types
- source precedence types
- candidate narrowing types
- adjudicator trait
- deterministic adjudicator
- active packet types
- packet rendering/tag/schema constants

Invariant:

- after this PR, typed broker law must not be duplicated across core helper structs and the new crate

Also require:

- concrete fail-closed conflict law
- deterministic candidate ordering
- stable adjudication output for identical inputs
- packet budget types that keep the overlay small

Do not add the `codex-core` dependency edge yet unless tests in this PR truly require it.

### PR S2 — Runtime Adapter And Prompt-Only Overlay Wiring

Purpose:

- connect the new crate to the live turn path without making the packet durable

Add:

- `core/src/semantic_broker_runtime.rs`

Update:

- [turn.rs](/home/rose/work/codex/fork/codex-rs/core/src/session/turn.rs)

Rules:

- inject the active packet after current prompt candidates are known
- inject exactly one broker-owned overlay item after `History::for_prompt(...)`
- append it to the end of the prompt input for the active request
- keep `build_initial_context(...)` unchanged
- do not route through context-maintenance or `History::for_prompt(...)`
- do not insert it between a call item and its output item
- apply the same prompt-only overlay to each sampling-loop prompt build during the active turn

This is also the PR that should:

- add the `codex-core` dependency on `codex-semantic-broker`
- enforce feature-gated default-off behavior

### PR S3 — Single-Source Packet Rendering Contract

Purpose:

- ensure no duplicate packet tag/schema/rendering contract remains outside the broker crate

Move or centralize:

- tag constant
- schema string/version
- render helpers
- packet-recognition helper for tests/debugging

Packet contract ownership should already start in S1.

So S3 is cleanup/verification only:

- remove any temporary duplicate constants
- ensure core consumes the broker-owned contract exclusively

### PR S4 — Cleanup And Narrowing

Purpose:

- reduce any leftover second-doctrine tests and overbroad exports after the live seam is stable

Expected cleanup:

- narrow public exports to the true adapter seam
- keep core tests focused on wiring/order, not broker law
- keep registry/adjudication tests with the broker crate

## Completion Criteria

This family is ready to stop when:

- typed broker law lives in `codex-semantic-broker`
- the runtime adapter hook is in `run_sampling_request(...)`
- feature-off behavior is unchanged
- feature-on behavior adds exactly one prompt-only packet
- the active packet is prompt-only and not durable
- registry/candidate/adjudication/packet ownership is single-sourced
- registry conflicts are deterministic and tested
- governance layering remains separate from broker resolution
- core acts as runtime adapter instead of semantic owner
