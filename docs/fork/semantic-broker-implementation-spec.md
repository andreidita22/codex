# Semantic Broker Implementation Spec

This is the family-level implementation spec for the semantic-broker extraction described in [semantic-broker-extraction-plan.md](/home/rose/work/codex/fork/docs/fork/semantic-broker-extraction-plan.md).

The worked-example reference pattern is:

- context-maintenance for semantic ownership extraction
- agent-observability for hot-file adapter reattachment

## Family Invariant

Semantic broker law should be owned by one dedicated module.

That means:

- registry structure and precedence law belong to the broker crate
- candidate narrowing and adjudication belong to the broker crate
- active packet schema and rendering contract belong to the broker crate
- runtime/session code may gather inputs and inject the packet, but must not become a second broker law surface

`v0` remains intentionally narrow:

- prompt-only packet
- deterministic adjudicator
- no durable history mutation
- no protocol changes

Recommended rollout shape:

- feature key: `semantic_broker`
- default: off
- when disabled, the core adapter returns no overlay and prompt behavior remains unchanged

## PR S0 — Prompt-Ingress Behavior Locks

### Purpose

Freeze the current prompt-ingress seam before the broker is introduced.

### Required locks

- `build_initial_context(...)` remains the owner of initial developer/contextual-user bundling
- governance prompt-layering remains a distinct developer-side packet
- `run_sampling_request(...)` remains the seam where prompt input becomes model-visible
- current context-maintenance behavior remains unchanged

### Test layers

Core/session tests:

- prompt assembly ordering
- initial-context bundle behavior
- no accidental persistence of prompt-only overlays

Governance-adjacent tests:

- governance layering still appears where it does today
- no hidden merge of governance and broker semantics

S0 should not add production overlay machinery.

If needed, use:

- seam inventory / ordering tests only
- or a test-only fake overlay helper that is removed or replaced in S2

### Scope guard

This PR should not introduce the new crate yet.

## PR S1 — Crate Creation And Typed Broker Law

### Purpose

Create `codex-semantic-broker` and move typed broker law into it in one copy-free step.

### New crate

- `codex-rs/semantic-broker`

### Required workspace wiring

- add `"semantic-broker"` to workspace members
- add `codex-semantic-broker = { path = "semantic-broker" }` to workspace dependencies
- do not introduce reverse dependency from the new crate to `codex-core`

Only add the dependency in `core/Cargo.toml` once S2 actually wires the live seam.

### Required initial public surface

The initial public API should stay narrow and typed. It should include:

- schema/operator ID newtypes
- registry snapshot types
- source precedence/conflict types
- candidate-set types
- adjudicator trait
- deterministic adjudicator implementation
- broker resolution types
- confidence/separation types
- lexical/clarify event types
- active packet types
- packet rendering contract surface

The broker crate should own packet contract items from the start:

- packet tag
- schema version string
- render helper
- recognition helper for tests/debugging

### Day-1 API shape

The public API should already reflect bounded selection over typed schema candidates.

Preferred shape:

```rust
pub fn build_candidate_set(
    input: &BrokerInput,
    registry: &SchemaRegistrySnapshot,
) -> CandidateSet;

pub fn adjudicate_candidates(
    input: &BrokerInput,
    candidates: &CandidateSet,
    adjudicator: &dyn SchemaAdjudicator,
) -> BrokerResolution;

pub fn build_active_packet(
    input: &BrokerInput,
    resolution: &BrokerResolution,
) -> ActiveContextPacket;
```

Avoid shaping the crate around:

- one monolithic `resolve()` call that hides registry/candidate structure
- one hard-coded enum-only verb matcher

Future model-assisted adjudication must not create a dependency from `codex-semantic-broker` to `codex-core`.

The broker crate may later own:

- model adjudication request schema
- model adjudication output validation

while the core adapter performs any actual model call.

### Registry precedence rule

This PR must define precedence/conflict behavior even if `v0` uses only builtin schemas.

At minimum it must make explicit:

- source families
- ordering rule
- same-ID collision behavior
- conflict event or rejection behavior

Preferred `v0` shape:

```rust
pub enum RegistrySourceFamily {
    Builtin,
    Project,
    UserApproved,
    SkillProvided,
    GovernanceProvided,
}

pub enum RegistryConflictKind {
    DuplicateSchemaId,
    DuplicateOperatorId,
    AliasTargetConflict,
    InvalidSourceOverride,
}

pub enum RegistryConflictResolution {
    RejectSnapshot,
    ShadowedByHigherPrecedence,
    IdenticalDuplicateIgnored,
}
```

`v0` behavior:

- only `Builtin` loads
- conflicting duplicates reject the snapshot
- identical duplicates may only be ignored if explicitly tested
- no silent precedence override

### Determinism rule

The deterministic adjudicator must satisfy:

- same `BrokerInput` + same `SchemaRegistrySnapshot` => same `CandidateSet`
- same `BrokerInput` + same `CandidateSet` => same `BrokerResolution`
- candidate ordering is stable
- ties become ambiguity, not incidental winner selection
- score comparisons use explicit stable rules

### Clarify rule for `v0`

`v0` does not short-circuit the main orchestrator into a broker clarification workflow.

So ambiguous or low-confidence resolution should behave as:

- no selected schema when confidence is unresolved
- optional ambiguity candidates
- optional clarify recommendation
- no `clarify_required` behavior that core must enforce

### Scope guard

Keep `v0` adjudication deterministic.

Do not add:

- model-based selection
- interactive authoring implementation
- durable packet persistence

## PR S2 — Runtime Adapter And Prompt Injection

### Purpose

Connect the broker crate to the live turn path while keeping the packet prompt-only.

### Required adapter file

- `core/src/semantic_broker_runtime.rs`

### Required runtime updates

Update:

- [turn.rs](/home/rose/work/codex/fork/codex-rs/core/src/session/turn.rs)

The runtime adapter should gather:

- current user-turn prompt candidates
- current tool surface
- current track/session source hints
- available registry snapshot inputs

Then it should:

- build candidates
- run deterministic adjudication
- build the active packet
- render one prompt-only overlay item
- inject that overlay into the prompt input immediately before `build_prompt(...)`

Precise ordering rule:

- start from the prompt input returned by `History::for_prompt(...)`
- append exactly one broker-owned overlay item to the end of that prompt input
- then call `build_prompt(...)`
- repeat that for each sampling-loop prompt build during the active turn
- never pass the overlay item back through `History::for_prompt(...)`
- never insert it between a call item and its output item

### Boundary rules

Do not:

- write broker packet into durable session history
- route through `History::for_prompt(...)`
- feed broker packet into context-maintenance artifact generation
- merge broker semantics into governance prompt layers

The runtime side may translate current session/tool/track state into broker input, but it must not re-author:

- precedence law
- candidate narrowing law
- adjudication law
- packet schema meaning

Core may extract:

- current turn text
- session source / subagent source
- visible tool names
- relevant feature/config state
- existing runtime facts already available at the turn boundary

Core must not:

- score schemas
- choose the winning procedure/operator
- apply alias law
- decide confidence
- invent packet fields

Governance remains an input class, not a crate dependency:

- `codex-semantic-broker` must not depend on `core/src/governance`
- core may translate governance-derived facts into `BrokerInput`

### Test layers

Core adapter/runtime tests:

- packet inserted at the intended seam
- packet is prompt-visible for the active request
- packet is not durably persisted
- governance packet ordering is not broken
- feature off => no packet and no prompt change
- feature on => exactly one packet
- history inspection after the turn still shows no durable packet
- context-maintenance source paths still do not see the packet

## PR S3 — Single-Source Packet Contract

### Purpose

Ensure the active packet’s model-visible contract is owned by the broker crate.

### Canonical items

- packet tag constant
- packet schema/version string
- render helper
- test/debug recognition helper

### Integration rule

Core should consume these from `codex-semantic-broker`.

Avoid leaving:

- tag constants in core
- schema strings in adapter tests
- duplicate packet rendering logic in prompt assembly

### Scope guard

Do not move general prompt-building ownership into the broker crate.

`build_prompt(...)` remains core-side.

The recognition helper is for:

- tests
- debug assertions
- future exclusion hooks

It should not become a `v0` runtime routing dependency.

## PR S4 — Cleanup And Narrowing

### Purpose

Finish the family by reducing leftover second-doctrine tests and overbroad exports.

### Expected cleanup

- narrow broker-crate public exports to the actual adapter seam
- keep core tests focused on insertion and ordering behavior
- keep registry/adjudication/packet-law tests in the broker crate

### Scope guard

This PR should not introduce semantic changes.

## Cross-Cutting Rules

### No generic fork hub

Do not turn this family into a generic broker for all fork modules.

The right shape remains:

- hot upstream runtime files
- thin adapters
- domain-specific semantic crate

### No compaction/history integration in `v0`

The broker is not a context-maintenance feature in this slice.

Do not:

- produce thread-memory inputs
- produce continuation-bridge inputs
- make the packet durable

### No protocol expansion in `v0`

Do not extend protocol models to carry the packet yet.

The active packet may be rendered in a broker-owned tagged prompt form during `v0`.

### Packet should stay mostly refs + bindings

The active packet should primarily carry:

- schema/operator refs
- bindings
- confidence
- clarify/evolution events

Avoid growing it into a second giant semantic document.

Add an explicit small packet budget in `v0`:

- no full schema bodies
- no memory bodies
- no full history excerpts
- short rationale only

### Interactive schema authoring is future-adjacent only

Keep the future ontology ready for:

- schema authoring
- draft/proposed/approved lifecycle
- alias-vs-new-schema distinction

But do not implement that workflow in this family unless a later dedicated follow-up explicitly takes it on.

## Completion Criteria

This family is ready to stop when:

- typed broker law lives in `codex-semantic-broker`
- the runtime seam is a small adapter near `run_sampling_request(...)`
- feature-off behavior is unchanged
- feature-on behavior adds exactly one prompt-only packet
- the active packet is prompt-only and not durable
- registry/candidate/adjudication/packet ownership is single-sourced
- registry conflicts are deterministic and tested
- governance layering remains separate
- core acts as adapter/runtime wiring instead of semantic owner
