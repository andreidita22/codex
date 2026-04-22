## Verdict

The v0 semantic-broker specs are directionally correct and implementable. They preserve the right geometry: dedicated semantic crate, prompt-ingress insertion near `run_sampling_request`, prompt-only packet, no durable history mutation, no protocol expansion, and no merge with governance/context-maintenance. That is the right first slice.  

The specs need tightening before implementation. The main risk is not that the design is too ambitious; the risk is that the v0 documents still leave several “implementation-choice” gaps exactly where hidden law surfaces tend to appear: registry conflict behavior, clarify semantics, packet insertion ordering, model-adjudicator extensibility, and duplicate packet-rendering ownership. 

## What is strong

The strongest parts should remain unchanged.

The broker is correctly scoped as a **prompt-ingress semantic module**, not a compaction feature, memory feature, history-normalization feature, or generic fork hub. The audit review explicitly accepts that the broker should be a dedicated prompt-ingress semantic module with a prompt-only active packet, and that the second architecture draft correctly reframes it as bounded adjudication over a typed schema registry rather than a dead-end verb matcher. 

The extraction plan’s ownership split is also right:

```text
codex-semantic-broker
  owns registry / candidates / adjudication / packet law

codex-core
  gathers runtime inputs and injects prompt-only overlay
```

That matches the fork pattern already used for context maintenance and observability. The plan is explicit that the broker should not own history normalization, context-maintenance route law, durable retention, session lifecycle, or tool execution. 

The v0 implementation spec is also right to require day-1 bounded-selection-shaped APIs:

```rust
build_candidate_set(...)
adjudicate_candidates(...)
build_active_packet(...)
```

rather than a single opaque `resolve()` function. That is the correct guardrail against fossilizing v0 into a hard-coded operator matcher. 

## Required amendments before implementation

### 1. Resolve the v0/v1 naming mismatch

The specs repeatedly say “v1,” while the current request and likely implementation family are “v0.” This is not just cosmetic. It affects packet schema names, feature names, tests, and rollout expectations.

Pick one convention before coding.

Recommended:

```text
implementation family: semantic broker v0
packet schema: codex.semantic_broker.active_packet.v0
future bounded model version: v1 or later
```

Then update all spec wording that currently says:

```text
V1 remains intentionally narrow
v1 prompt-only packet
v1 adjudication
```

to:

```text
v0 remains intentionally narrow
v0 prompt-only packet
v0 deterministic adjudication
```

The current architecture can still call future model-assisted selection “v1.”

### 2. Make registry conflict behavior concrete, not merely required

The specs correctly say registry precedence/conflict handling must exist even if v0 has builtin schemas only.   But they do not define enough behavior for implementation.

For v0, use a strict law:

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

Then define v0 behavior:

```text
v0 only loads Builtin schemas.

Within Builtin:
- duplicate schema ID with different content => reject registry snapshot
- duplicate operator ID with incompatible definition => reject registry snapshot
- duplicate alias targeting different objects => reject registry snapshot
- exact duplicate content may be ignored only if explicitly tested
```

Do **not** implement silent precedence override in v0. Future source precedence can be added later, but v0 should be fail-closed.

This should be a hard requirement in PR S1, not just a design note.

### 3. Do not add the core dependency in S1 unless S1 actually wires core

The implementation spec says PR S1 should add `codex-semantic-broker` to workspace dependencies and add a dependency in `core/Cargo.toml`. 

That is premature if S1 only creates the crate and owns broker law.

Better sequence:

```text
S1:
  add workspace member
  add workspace dependency
  implement broker crate
  no core dependency yet unless tests need it

S2:
  add core dependency
  add core/src/semantic_broker_runtime.rs
  wire turn.rs
```

This keeps S1 as pure semantic ownership and avoids adding unused core dependency before the live seam exists.

### 4. Collapse S3 into S1/S2 or redefine it as cleanup only

The current spec says S1 should include the packet rendering contract surface, while S3 later single-sources the packet tag/schema/rendering contract.  That creates ambiguity: either the broker owns rendering in S1, or S3 is cleaning up duplicate temporary rendering.

Do not allow temporary packet tags/schema strings in core.

Recommended revision:

```text
S1:
  broker crate owns packet tag, schema version, render helper, recognition helper

S2:
  core consumes broker-owned render helper

S3:
  optional cleanup only, verifying no duplicate constants/schema strings remain
```

If S3 remains a real PR, it should not be the first time packet rendering becomes single-sourced.

### 5. Specify packet insertion ordering precisely

The specs say inject immediately before `build_prompt(...)`, which is correct.  But implementation needs exact ordering.

Recommended v0 rule:

```text
Take the prompt input returned by History::for_prompt(...).
Append exactly one broker-owned developer-role packet ResponseItem to the end of that prompt input.
Then call build_prompt(...).
Do not pass the packet back through History::for_prompt(...).
Do not record it in session history.
Apply the same overlay to each sampling-loop prompt input for the active turn.
```

Add one invariant:

```text
The overlay must not be inserted between a call item and its output item.
```

Appending after `for_prompt(...)` should satisfy this, assuming `for_prompt` already normalizes tool/call ordering.

### 6. Clarify ambiguous-resolution behavior in v0

The long-term design wants broker-driven clarification. The implementation spec correctly keeps interactive authoring and model-based clarification out of this family.  But v0 still needs to say what happens if the deterministic adjudicator is ambiguous.

Do **not** emit a packet that says `clarify_required: true` unless core actually short-circuits main orchestration and asks the broker clarification. That is not in v0 scope.

Recommended v0 behavior:

```rust
pub enum ClarifyDisposition {
    None,
    Recommended,
    RequiredButNotEnforced,
}
```

But for v0 packet behavior:

```text
If confidence is below threshold:
  emit no selected_schema
  emit confidence decision = Unresolved
  optionally include clarify_recommended

If top candidates are too close:
  emit ambiguity candidates
  emit clarify_recommended

Do not mark clarify_required in v0.
```

Future slice can introduce:

```rust
BrokerDecision::Clarify(ClarifyRequest)
```

and core short-circuit behavior.

### 7. Define deterministic adjudicator output invariants

The spec says deterministic adjudicator, but not enough about determinism.

Add explicit invariants:

```text
Same BrokerInput + same SchemaRegistrySnapshot => same CandidateSet and BrokerResolution.
Candidate ordering is stable.
Ties are represented as ambiguity, not resolved by hash/order accident.
Floating score comparisons use explicit epsilon or integer scores.
```

This matters because the packet will become model-visible. Non-deterministic packet changes will create hard-to-debug prompt drift.

### 8. Keep `SchemaAdjudicator` future-safe without forcing model calls into the broker crate

The proposed trait is fine for deterministic v0:

```rust
pub trait SchemaAdjudicator {
    fn adjudicate(...) -> BrokerResolution;
}
```

But future bounded model adjudication cannot make `codex-semantic-broker` depend on `codex-core` or model clients.

So add this rule now:

```text
The broker crate may define model-adjudication request/response schemas and validators.
The core runtime adapter performs any future model call.
The broker crate validates the model’s selected schema/operator against CandidateSet.
```

Future shape:

```rust
pub struct ModelAdjudicationRequest { ... }
pub struct ModelAdjudicationOutput { ... }

pub fn build_model_adjudication_request(...) -> ModelAdjudicationRequest;

pub fn validate_model_adjudication_output(
    candidates: &CandidateSet,
    output: ModelAdjudicationOutput,
) -> BrokerResolution;
```

Do not design a future `BoundedModelAdjudicator` that requires `codex-semantic-broker -> codex-core`.

### 9. Specify `BrokerInput` construction boundaries

The runtime adapter is allowed to gather current state, but it must not become a second broker. The specs say this generally.  Add concrete v0 boundaries:

Core may extract:

```text
latest/current user turn text
session source / subagent source
visible tool names
prompt-input item roles
known context-maintenance artifact tags if exposed by existing helpers
feature/config state
```

Core must not:

```text
score schemas
choose procedure/operator
apply alias law
decide confidence
decide registry precedence
invent packet fields
```

For current user text, prefer an explicit field:

```rust
pub struct BrokerInput {
    pub current_turn_text: Option<String>,
    ...
}
```

Do not require the broker crate to parse the full `ResponseItem` history to guess the current turn.

### 10. Keep governance as an input class, not a dependency

The specs correctly say governance layering is adjacent but separate.  Make that dependency rule explicit:

```text
codex-semantic-broker must not depend on core/src/governance.
core may translate governance-derived facts into BrokerInput candidates.
```

If broker authority classes resemble governance classes, define broker-local types and adapter translations. Do not import `core::governance::NormativeForce` into the broker crate.

### 11. Add packet-size and content budget constraints

The specs say the packet should stay mostly refs + bindings, not a giant document.  Good, but implementation needs a budget.

Add v0 limits:

```rust
pub struct PacketBudget {
    pub max_active_artifacts: usize,
    pub max_dormant_artifacts: usize,
    pub max_rationale_chars: usize,
    pub max_total_packet_chars: usize,
}
```

Default v0:

```text
small enough to prove architecture:
- no full schema bodies
- no memory bodies
- no full history excerpts
- short rationale only
```

This prevents the prompt-only overlay from becoming a second context soup.

### 12. Feature gate and default-off behavior should be explicit

The implementation spec implies v0 narrowness but does not name the feature gate. Add it.

Recommended:

```rust
Feature::SemanticBroker
```

Feature key:

```text
semantic_broker
```

Stage:

```text
UnderDevelopment
```

Default:

```text
off
```

Core adapter should return `Ok(None)` when disabled.

Tests should prove:

```text
feature off => no packet, no prompt change
feature on => exactly one packet, prompt-only
```

### 13. S0 needs a realistic test strategy

S0 says it should test prompt overlay behavior without introducing the new crate.  That is awkward unless a test-only fake overlay seam is introduced.

Two acceptable options:

Option A — make S0 purely behavioral/seam inventory:

```text
test build_initial_context / governance ordering / current prompt construction unchanged
do not test broker overlay yet
```

Option B — add a tiny test-only prompt overlay helper in core:

```rust
#[cfg(test)]
fn append_test_prompt_overlay(...)
```

Then remove or replace it in S2.

Do not add production overlay machinery in S0. That would blur S0 and S2.

### 14. Define “prompt-only” as a testable invariant

The specs repeatedly say prompt-only and not durable.  Make it testable:

```text
After one broker-enabled sampling request:
- prompt input passed to build_prompt contains packet
- sess.clone_history().await.raw_items() does not contain packet
- subsequent ContextManager::for_prompt(...) without overlay does not contain packet
- context-maintenance artifact generation source input does not contain packet
```

If direct access to `raw_items()` is not available in tests, use the narrowest equivalent history inspection helper.

### 15. Packet recognition helper should be test/debug only

S3 calls for a packet-recognition helper.  That is useful, but do not let core use it to make runtime semantic decisions in v0.

Recommended:

```rust
pub fn is_active_context_packet_text(text: &str) -> bool
```

or:

```rust
pub fn response_item_contains_active_context_packet(item: &ResponseItem) -> bool
```

Use cases:

```text
tests
debug assertions
future source-exclusion hooks
```

Not v0 runtime routing.

## PR-by-PR review

### PR S0 — Prompt-ingress behavior locks

Proceed, but narrow.

Keep:

* `build_initial_context(...)` remains separate.
* Governance layering remains separate.
* `run_sampling_request(...)` remains the intended seam.
* Context-maintenance tests remain unchanged. 

Change:

* Do not require proof of broker overlay insertion before broker overlay exists.
* Either make S0 an inventory/behavior-lock PR or add a test-only fake overlay helper.

Add:

* a test or assertion that prompt-only additions at the sampling boundary do not mutate session history, if practical.

### PR S1 — Crate creation and typed broker law

Proceed with amendments.

Required additions:

* concrete conflict law;
* deterministic-resolution invariants;
* packet budget types;
* static builtin registry fixture;
* tests for duplicate schema ID rejection;
* tests for candidate-set stability.

Move core dependency to S2 unless S1 actually uses it.

Do not implement file-backed schema loading yet.

### PR S2 — Runtime adapter and prompt injection

This is the highest-risk v0 PR.

Require:

* feature gate default off;
* `core/src/semantic_broker_runtime.rs`;
* exactly one overlay packet when enabled;
* no durable history mutation;
* no governance merge;
* no context-maintenance path involvement;
* injection after `for_prompt(...)`, before `build_prompt(...)`.

Core adapter may collect inputs. It must not score, select, or adjudicate.

### PR S3 — Single-source packet contract

Make this either redundant-by-design or cleanup-only.

Do not allow duplicate packet tag/schema constants to exist after S2.

If S3 stays, its purpose should be:

```text
verify and remove temporary test constants / overly broad render helpers
```

not:

```text
finally move packet rendering ownership into broker crate
```

### PR S4 — Cleanup and narrowing

Good.

Add explicit cleanup targets:

* no core tests asserting broker route/schema law;
* no core-local packet JSON expectations except via broker helper;
* public broker exports limited to registry/adjudication/packet/render adapter seam;
* no test-only broad APIs left public.

## Changes to avoid

Do not turn this into a governance refactor. Governance prompt layering and semantic ingress resolution are adjacent but distinct; the audit review explicitly calls this out. 

Do not add protocol-level packet variants in v0. The extraction plan explicitly excludes protocol-level packet variants. 

Do not make broker packets durable in v0. The extraction plan and implementation spec both exclude durable history mutation and context-maintenance integration.

Do not implement interactive schema authoring in this family. The docs are correct to keep it future-adjacent only.

Do not build a generic fork hub. The accepted design is a domain-specific semantic crate plus thin core adapter. 

## Recommended amended completion criteria

The family is done when:

```text
codex-semantic-broker exists and has no dependency on codex-core
registry/candidate/adjudication/packet law is owned there
registry conflicts are deterministic and tested
packet tag/schema/rendering are single-sourced there
feature-off behavior is unchanged
feature-on behavior adds exactly one prompt-only packet
packet is not recorded in session history
core adapter gathers inputs but does not adjudicate
governance layering remains separate
no context-maintenance behavior changes
no protocol model changes
```

## Final judgment

Proceed after amendments.

The v0 specs have the right architecture. The main corrections are to make the registry/conflict law concrete, clarify ambiguous-resolution behavior, make packet insertion and prompt-only invariants testable, and avoid a temporary split where packet rendering or schema constants exist in both core and the broker crate.

With those changes, v0 stays narrow while preserving the long-term path toward bounded model selection over repo-native schemas.
