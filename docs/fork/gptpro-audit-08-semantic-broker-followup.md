## 1. What the previous design got right

The baseline geometry should remain unchanged:

```text
user turn accepted
  -> current context/history/tool candidates available
  -> semantic broker pass builds prompt-only active packet
  -> main orchestrator receives normal prompt input + broker overlay
  -> no durable history mutation in v1
```

The right insertion seam is still near `core/src/session/turn.rs::run_sampling_request`, after tools/base instructions are available and before `build_prompt(...)`. That gives the broker the current prompt candidates and tool surface without forcing broker law into `ContextManager::for_prompt`.

The module split also remains right:

```text
semantic-broker/
  codex-semantic-broker
  pure typed schema/resolution/packet law

core/src/semantic_broker_runtime.rs
  runtime adapter over Session, TurnContext, history, router, features
```

This follows the same fork pattern that worked for context maintenance and observability: semantic law lives in a dedicated module/crate; `codex-core` remains adapter/runtime wiring. The observability extraction explicitly adopted that pattern: progress/reducer law moved to a semantic crate, while runtime waiting, target resolution, and lifecycle orchestration stayed in core.  The implementation invariant there is directly reusable: runtime code may translate and execute, but it must not become a second law surface. 

Also still correct:

* v1 packet is prompt-only, not durable history.
* No protocol variant in v1.
* No `ContextManager::for_prompt` rewrite.
* No compaction/thread-memory/continuation-bridge changes in v1.
* Broker packet should be typed and tagged, not hidden prose.
* Broker should not become a generic “fork hub.”

The correction is not about where the broker sits. The correction is about making the **registry/adjudication architecture extensible from day 1**, so v1 is not accidentally a dead-end hard-coded verb matcher.

---

## 2. Missing long-term concept: bounded semantic selection over pre-selected schemas

### Definition

A **bounded semantic model** here means:

```text
a constrained adjudicator operating over an explicitly supplied candidate set of typed schemas,
with fixed output schema, confidence/separation rules, and no authority to invent binding procedure law.
```

It is not:

* free conversational planning;
* arbitrary semantic improvisation;
* embedding-only nearest-neighbor retrieval;
* mining `memory.md` and treating it as procedure;
* a permanent six-entry Rust enum registry.

It is:

```text
repo-native schema pool
  -> candidate narrowing
  -> bounded semantic adjudication over candidates
  -> typed resolution result
  -> clarify/escalate if ambiguous
  -> active packet construction
```

### Candidate schema pool

The candidate pool is the set of available, valid, versioned schema artifacts that conform to a broker-owned meta-schema.

Sources can later include:

```text
built-in schemas in semantic-broker/
project-local schemas under repo-controlled paths
user-approved schema packs
skill-provided procedure schemas
organization/governance schemas
active track schemas
```

The broker does **not** scan arbitrary prose and decide it is procedure. It only adjudicates among schemas that have already passed registry validation.

### Candidate narrowing

Candidate narrowing should be deterministic and typed before any model adjudication.

Inputs:

* current user turn text;
* current active track;
* known task family;
* repo/project schema availability;
* attached file classes;
* visible tool surface;
* current operation class such as review, audit, implementation, verification;
* schema metadata: aliases, trigger predicates, expected artifacts, authority class, lifecycle.

Narrowing output:

```rust
pub struct CandidateSet {
    pub registry_version: RegistryVersion,
    pub candidates: Vec<SchemaCandidate>,
    pub excluded: Vec<ExcludedSchemaCandidate>,
    pub narrowing_witness: CandidateNarrowingWitness,
}
```

Only this narrowed candidate set is passed to the bounded adjudicator.

### Difference from flat retrieval

Flat retrieval says:

```text
query -> nearest documents -> paste docs into prompt
```

The broker should instead do:

```text
query -> valid schema candidates -> typed adjudication -> selected schema/procedure ID -> active packet
```

The selected result is a schema reference plus typed bindings, not copied prose. This matters because “memory says X” and “procedure schema requires X” must remain distinct.

### Difference from free semantic improvisation

Free improvisation lets the model invent an operator or synthesize a procedure because it sounds right.

The broker must not do that. If a term is novel, the broker emits a typed event:

```rust
LexicalEvolutionEvent::NovelOperatorProposed { ... }
```

or:

```rust
SchemaEvolutionProposal { ... }
```

It does not silently add procedure law.

### Architectural consequence

The broker crate API must be built around:

```text
registry snapshot
candidate set
adjudication result
packet construction
clarify/evolution events
```

not around:

```text
match user text against six hard-coded verbs
```

The deterministic resolver can be v1’s first adjudicator implementation, but the API should already look like the future bounded-selection pipeline.

---

## 3. Day-1 implications of that long-term vision

### 3.1 Broker API should accept a registry snapshot now

Instead of:

```rust
resolve_broker_packet(input: BrokerInput) -> BrokerDecision
```

as a monolithic call, use:

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

In v1:

```rust
DeterministicAdjudicator
```

implements `SchemaAdjudicator`.

Later:

```rust
BoundedModelAdjudicator
```

uses the same `CandidateSet` and returns the same typed `BrokerResolution`.

### 3.2 Do not make procedure IDs enum-only

Keep canonical operator classes as enums, but schema/procedure IDs should be stable strings or typed newtypes:

```rust
pub struct SchemaId(String);
pub struct ProcedureId(String);
pub struct OperatorId(String);
```

Canonical operators may be enum-backed:

```rust
pub enum CanonicalOperator {
    Review,
    Audit,
    Verify,
    Judge,
    Compare,
    Implement,
    Debug,
    Summarize,
    Plan,
}
```

But a workflow schema should reference:

```rust
operator_id: OperatorId
```

not only:

```rust
operator: CanonicalOperator
```

That preserves future schema families without recompiling the world for every workflow schema.

### 3.3 Packet should refer to schemas, not just prose summaries

The active packet should contain typed references:

```rust
pub struct ActiveContextPacket {
    pub packet_schema: PacketSchemaVersion,
    pub packet_id: PacketId,

    pub resolution: BrokerResolutionSummary,
    pub active_schema: Option<ResolvedSchemaRef>,
    pub active_operator: Option<ResolvedOperatorRef>,
    pub active_track: TrackRef,

    pub authority_bindings: Vec<AuthorityBinding>,
    pub artifact_bindings: Vec<ArtifactBinding>,
    pub clarify: Option<ClarifyRequest>,
    pub lexical_events: Vec<LexicalEvolutionEvent>,
    pub witness: BrokerWitness,
}
```

Avoid:

```rust
focus_summary: String
```

as the primary meaning carrier. A short focus summary is fine, but it must be secondary.

### 3.4 Confidence must include separation, not only scalar confidence

Add:

```rust
pub struct ResolutionConfidence {
    pub selected_score: f32,
    pub runner_up_score: Option<f32>,
    pub separation: Option<f32>,
    pub threshold: f32,
    pub decision: ConfidenceDecision,
}
```

Because the critical broker question is not only “is this plausible?” but:

```text
is this candidate sufficiently better than the alternatives?
```

### 3.5 Alias table must be distinct from schema registry

Day 1 should already separate:

```text
canonical operator set
schema registry
alias/synonym table
```

Do not bake aliases into the only procedure registry structure.

Use:

```rust
pub struct AliasRegistrySnapshot {
    pub aliases: Vec<AliasEntry>,
}

pub struct AliasEntry {
    pub alias: String,
    pub target: AliasTarget,
    pub status: AliasStatus,
    pub source: AliasSource,
    pub confidence: f32,
}
```

Alias statuses:

```rust
pub enum AliasStatus {
    Canonical,
    Approved,
    Proposed,
    Rejected,
    Deprecated,
}
```

In v1 the alias registry can be static. The type separation prevents later alias evolution from mutating canonical schema definitions.

### 3.6 Candidate descriptors should be typed now

Day-1 candidate descriptors should not be just strings:

```rust
pub struct SchemaCandidate {
    pub schema_id: SchemaId,
    pub family: SchemaFamily,
    pub operator: OperatorId,
    pub aliases_hit: Vec<AliasHit>,
    pub expected_artifacts: Vec<ArtifactRequirement>,
    pub authority_requirement: AuthorityRequirement,
    pub lifecycle: SchemaLifecycle,
    pub score_features: ScoreFeatures,
}
```

This lets the deterministic v1 resolver and later bounded model share the same substrate.

---

## 4. Schema registry architecture

### 4.1 Four distinct layers

The registry architecture should distinguish these layers:

```text
1. Canonical operator meta-schema
2. Schema instances / procedure schemas / workflow schemas
3. Alias and synonym tables
4. Runtime registry snapshot
```

Do not collapse them.

### 4.2 Canonical operator meta-schema

This is broker-owned law.

It defines the admissible shape of operator families:

```rust
pub struct OperatorMetaSchema {
    pub version: MetaSchemaVersion,
    pub canonical_operators: Vec<CanonicalOperatorDef>,
    pub admissible_authority_classes: Vec<AuthorityClass>,
    pub admissible_lifecycle_markers: Vec<LifecycleMarker>,
    pub admissible_artifact_classes: Vec<ArtifactClass>,
    pub required_schema_fields: Vec<SchemaFieldDef>,
}
```

Canonical operator definition:

```rust
pub struct CanonicalOperatorDef {
    pub operator_id: OperatorId,
    pub family: OperatorFamily,
    pub description: &'static str,
    pub required_inputs: Vec<ArtifactRequirement>,
    pub allowed_outputs: Vec<ArtifactClass>,
    pub clarify_policy: ClarifyPolicy,
}
```

Example families:

```rust
Review
Audit
Verify
Judge
Compare
Implement
Debug
Plan
Summarize
SchemaAuthoring
```

The canonical meta-schema is versioned and hard-typed. It changes slowly.

### 4.3 Schema instances / workflow schemas

Schema instances are repo-native artifacts conforming to the meta-schema.

Example:

```rust
pub struct WorkflowSchema {
    pub schema_id: SchemaId,
    pub version: SchemaVersion,
    pub title: String,

    pub operator_id: OperatorId,
    pub family: SchemaFamily,

    pub aliases: Vec<String>,
    pub trigger_predicates: Vec<String>,

    pub expected_inputs: Vec<ArtifactRequirement>,
    pub output_contract: OutputContract,

    pub authority_policy: AuthorityPolicy,
    pub lifecycle_policy: LifecyclePolicy,
    pub track_policy: TrackPolicy,
    pub clarify_policy: ClarifyPolicy,

    pub prompt_packet_template: PacketTemplateRef,
    pub validation: SchemaValidationRules,
}
```

Storage options:

```text
semantic-broker/schemas/builtin/*.schema.toml
.codex/semantic-broker/schemas/*.schema.toml
.codex/semantic-broker/aliases.toml
```

Use JSON/TOML only if the schema is validated into typed Rust structs before use. The model should never consume raw schema files as authoritative law without validation.

### 4.4 Alias/synonym tables

Alias entries are not schema definitions.

Example:

```rust
pub struct AliasEntry {
    pub alias: String,
    pub target: AliasTarget,
    pub status: AliasStatus,
    pub scope: AliasScope,
    pub source: AliasSource,
    pub created_at: Option<Timestamp>,
    pub rationale: Option<String>,
}
```

Targets:

```rust
pub enum AliasTarget {
    Operator(OperatorId),
    Schema(SchemaId),
    ArtifactClass(ArtifactClass),
}
```

Scopes:

```rust
Global
User
Repo
Track
Session
```

Statuses:

```rust
Approved
Proposed
Rejected
Deprecated
```

The broker can use approved aliases automatically. Proposed aliases can increase score but should not silently alter canonical meaning.

### 4.5 Runtime registry snapshot

At runtime, the core adapter should provide:

```rust
pub struct SchemaRegistrySnapshot {
    pub meta_schema_version: MetaSchemaVersion,
    pub registry_version: RegistryVersion,
    pub operators: Vec<CanonicalOperatorDef>,
    pub schemas: Vec<WorkflowSchema>,
    pub aliases: AliasRegistrySnapshot,
    pub source_index: Vec<RegistrySource>,
}
```

The snapshot is immutable for one broker decision.

This is important for reproducibility:

```text
same turn + same registry snapshot -> same candidate set / decision, unless model adjudicator is explicitly enabled
```

### 4.6 Candidate filtering

Filtering should proceed in typed stages:

```text
registry snapshot
  -> filter by enabled schema families
  -> filter by current operation class
  -> filter by active track scope
  -> filter by artifact availability
  -> score lexical/operator matches
  -> build bounded candidate set
```

Sketch:

```rust
pub fn narrow_candidates(
    input: &BrokerInput,
    registry: &SchemaRegistrySnapshot,
    limits: CandidateLimits,
) -> CandidateSet
```

Limits:

```rust
pub struct CandidateLimits {
    pub max_candidates: usize,
    pub min_lexical_score: f32,
    pub include_weak_track_matches: bool,
}
```

The bounded model never sees the entire schema corpus. It sees the narrowed set plus exact typed metadata.

---

## 5. Lexical evolution / synonym handling

Natural-language synonymy must be first-class and typed.

### 5.1 Case A: high-confidence synonym

Example:

```text
“arbiter this patch”
```

Candidate analysis:

```text
arbiter ≈ judge
strong context: evaluation/adjudication
schema candidate: judge.policy_review
separation high
```

Broker result:

```rust
LexicalResolution::KnownOrHighConfidenceAlias {
    observed_term: "arbiter",
    target: AliasTarget::Operator(OperatorId("judge")),
    confidence: 0.88,
    alias_status: AliasStatus::Proposed,
    adoption: AliasAdoption::Suggest,
}
```

Packet behavior:

* select the canonical operator if confidence/separation are sufficient;
* include lexical event in packet witness;
* optionally ask for alias confirmation only if policy requires it.

No silent canonical mutation.

### 5.2 Case B: medium-confidence ambiguous synonym

Example:

```text
“assess this”
```

Could mean:

```text
review
audit
verify
judge
```

Broker result:

```rust
BrokerResolution::Ambiguous {
    candidates: Vec<ScoredSchemaCandidate>,
    clarify: ClarifyRequest {
        question: "Do you want a review-only pass, a verification pass, or an architecture audit?",
        options: [...]
    }
}
```

No main-orchestrator free interpretation. The broker either emits a clarify packet or short-circuits later when clarify UX is implemented.

### 5.3 Case C: true semantic novelty

Example:

```text
“triangulate this architecture against the ritual contracts”
```

If no approved schema/operator is close enough:

```rust
LexicalEvolutionEvent::NovelSemanticPredicate {
    observed_term: "triangulate",
    context_excerpt: "...",
    nearest_candidates: Vec<ScoredSchemaCandidate>,
    recommended_action: SchemaEvolutionAction::ProposeNewSchema,
}
```

Behavior:

* do not invent a procedure silently;
* optionally route to schema-authoring mode;
* otherwise ask a narrow clarification using existing candidates.

### 5.4 Typed events

Use explicit events:

```rust
pub enum LexicalEvolutionEvent {
    AliasObserved {
        observed_term: String,
        target: AliasTarget,
        confidence: f32,
        status: AliasStatus,
    },
    AliasClarificationNeeded {
        observed_term: String,
        candidates: Vec<AliasTargetScore>,
    },
    AliasAdoptionProposed {
        observed_term: String,
        target: AliasTarget,
        scope: AliasScope,
        rationale: String,
    },
    NovelSemanticPredicate {
        observed_term: String,
        nearest_candidates: Vec<AliasTargetScore>,
        recommendation: SchemaEvolutionAction,
    },
}
```

This prevents semantic drift from being hidden inside packet prose.

---

## 6. Interactive schema-creation mode

### 6.1 What it is

Schema creation mode is a bounded workflow for authoring valid broker schemas.

It is not:

```text
“paste markdown into a skill file”
```

It is:

```text
guided creation of a WorkflowSchema that validates against OperatorMetaSchema
```

The agent in this mode is bounded by the schema-authoring meta-schema. It asks targeted questions, drafts schema fields, validates structure, and distinguishes alias adoption from genuine schema creation.

### 6.2 Runtime geometry

Recommended future geometry:

```text
user requests schema creation / broker detects novel schema need
  -> broker selects schema_authoring workflow
  -> schema-authoring bounded mode starts
  -> user answers targeted questions
  -> draft WorkflowSchema created
  -> validator checks meta-schema conformance
  -> user approves
  -> schema stored as proposed or approved
  -> registry snapshot updated on next turn/session
```

This should be a broker-adjacent feature, not part of the main operational orchestrator.

### 6.3 Module ownership

Suggested structure:

```text
semantic-broker/
  src/meta_schema.rs
  src/schema_registry.rs
  src/schema_authoring.rs
  src/schema_validation.rs
  src/alias.rs
```

Core adapter:

```text
core/src/semantic_broker_runtime.rs
core/src/semantic_broker_schema_authoring.rs
```

The broker crate owns:

* meta-schema;
* workflow schema structs;
* validation;
* alias proposal semantics;
* schema-authoring state machine types.

Core owns:

* file writes;
* user interaction plumbing;
* config gates;
* approval UX;
* persistence locations.

### 6.4 Relation to skills

This should integrate with skills but not be identical to skills.

Skills today are operational instruction artifacts and tool-like capability packaging. Broker schemas are ingress/procedure mediation artifacts.

Possible relation:

```text
skill may provide workflow schemas
workflow schema may reference skill IDs
schema authoring may produce a skill-adjacent procedure schema
```

But a broker schema should not simply be another prompt file. It should validate against broker meta-schema and produce active packet semantics.

### 6.5 Relation to memory

Memory can suggest that a schema might be useful. Memory cannot install schema law.

Schema authoring artifacts should not live in `memory.md`. They should be stored under a governed schema registry location, such as:

```text
.codex/semantic-broker/schemas/
.codex/semantic-broker/aliases.toml
```

or a future configured schema-pack path.

### 6.6 Storage and versioning

Each schema should include:

```rust
pub struct SchemaHeader {
    pub schema_id: SchemaId,
    pub schema_version: SchemaVersion,
    pub meta_schema_version: MetaSchemaVersion,
    pub status: SchemaStatus,
    pub source: SchemaSource,
    pub created_by: Option<String>,
    pub reviewed_by: Option<String>,
}
```

Statuses:

```rust
Draft
Proposed
Approved
Deprecated
Rejected
```

Only approved schemas should participate automatically in broker resolution unless config explicitly enables proposed schemas.

---

## 7. Broker clarification UX

### 7.1 Clarification should be operational, not apologetic

Broker clarify questions should be narrow and schema-aware.

Good:

```text
“Do you mean Arbiter as a synonym of Judge in this workflow, or do you want to create a distinct Arbiter operator?”
```

Good:

```text
“Do you want Policy-27 full review-and-merge, or review-only?”
```

Bad:

```text
“I’m not sure what you mean. Can you clarify?”
```

### 7.2 Information to surface

The broker should expose enough canonical meaning for the user to choose:

```rust
pub struct ClarifyOption {
    pub id: String,
    pub label: String,
    pub target: ClarifyTarget,
    pub canonical_operator: Option<OperatorId>,
    pub schema_id: Option<SchemaId>,
    pub short_definition: String,
    pub consequences: Vec<String>,
}
```

Example:

```text
Option A: Judge
Meaning: adjudicate among alternatives and produce a decision.
Consequence: output includes ruling + rationale.

Option B: Review
Meaning: inspect and report issues without making final decision.
Consequence: output includes findings + recommendations.
```

Do not dump entire schema definitions into the clarification. Provide canonical summary and consequences.

### 7.3 Clarification result feeds typed state

A user answer should produce:

```rust
BrokerClarificationOutcome {
    pub selected_target: ClarifyTarget,
    pub alias_update: Option<AliasUpdateProposal>,
    pub schema_update: Option<SchemaEvolutionProposal>,
    pub active_resolution: BrokerResolution,
}
```

If user says:

```text
“Yes, Arbiter means Judge here.”
```

Then:

```rust
AliasUpdateProposal {
    alias: "arbiter",
    target: AliasTarget::Operator("judge"),
    scope: AliasScope::Repo or User,
    status: Proposed,
}
```

Not automatic permanent mutation unless policy allows it.

### 7.4 Avoid generic confusion

Clarify UX should trigger only for broker-relevant ambiguity:

* operator ambiguity;
* schema ambiguity;
* authority ambiguity;
* track ambiguity;
* alias adoption;
* schema creation/evolution.

It should not ask clarification for every underspecified task detail. The main orchestrator can handle ordinary task clarification after the broker packet is established.

---

## 8. Multi-track implications

### 8.1 Schema-aware active packet foregrounding

The broker should select schemas scoped to the active track, not the entire conversation blob.

Track state should include:

```rust
pub struct TrackBrokerState {
    pub track_id: TrackId,
    pub active_schema: Option<SchemaId>,
    pub active_operator: Option<OperatorId>,
    pub aliases_in_scope: Vec<AliasEntry>,
    pub dormant_schema_refs: Vec<SchemaId>,
    pub unresolved_clarifications: Vec<ClarifyRequestId>,
}
```

### 8.2 Dormant track suppression

If a previous track had a schema active, that schema should not remain active merely because its packet or summary exists in history.

Active packet should distinguish:

```rust
active_schema
dormant_schema_refs
suppressed_track_refs
```

The main orchestrator sees:

```text
This turn is using schema X.
Schemas Y and Z exist in dormant tracks but are not active unless foregrounded.
```

### 8.3 Candidate selection scoped to active track

Candidate narrowing should be:

```text
current user turn
+ active track state
+ directly referenced dormant track if any
+ current repo artifacts
```

not:

```text
all procedures ever mentioned in this session
```

This prevents parallel workstreams from half-occupying the model’s active attention.

### 8.4 Track-specific alias scope

Aliases can be scoped:

```text
session
track
repo
user
global
```

Example:

```text
In this project, “Arbiter” means “Judge.”
In another track, “arbiter” might refer to a specific internal tool.
```

The broker should not globally adopt aliases discovered in one track unless explicitly approved at a wider scope.

### 8.5 Compaction/rehydration implication

Later, compacted summaries should preserve:

```text
track ID
active schema ID
resolved operator
open clarifications
approved track-local aliases
```

They should not preserve raw broker packet prose as durable law. This is analogous to the context-maintenance hardening principle: deterministic policy should not be bypassed by accidentally treating old prompt artifacts as live law. The compaction hardening spec’s concern about preserving route/disposition law by construction is the same class of issue here. 

---

## 9. Suggested revisions to the v1 design

### 9.1 Add these abstractions now

Add in `codex-semantic-broker` v1:

```rust
SchemaRegistrySnapshot
OperatorMetaSchema
WorkflowSchema
AliasRegistrySnapshot
CandidateSet
SchemaCandidate
BrokerResolution
ResolutionConfidence
LexicalEvolutionEvent
```

Even if v1 populates them from static built-ins, the type shape should be the future shape.

### 9.2 Replace “procedure registry” with layered registry

Previous v1:

```text
small built-in ProcedureRegistry
```

Revised v1:

```text
SchemaRegistrySnapshot {
  meta_schema,
  builtin_workflow_schemas,
  builtin_aliases,
}
```

The built-in six procedures become built-in workflow schemas, not special-case Rust match arms.

### 9.3 Keep deterministic resolver, but make it an adjudicator implementation

V1 should use:

```rust
pub trait SchemaAdjudicator {
    fn adjudicate(
        &self,
        input: &BrokerInput,
        candidates: &CandidateSet,
    ) -> BrokerResolution;
}
```

Then:

```rust
DeterministicAdjudicator
```

is slice 1.

Later:

```rust
BoundedModelAdjudicator
```

drops in without changing core hook geometry.

### 9.4 Packet fields to make typed now

Change packet fields from summary-heavy to reference-heavy:

```rust
active_schema: Option<ResolvedSchemaRef>
active_operator: Option<ResolvedOperatorRef>
resolution_confidence: ResolutionConfidence
authority_bindings: Vec<AuthorityBinding>
artifact_bindings: Vec<ArtifactBinding>
lexical_events: Vec<LexicalEvolutionEvent>
clarify: Option<ClarifyRequest>
```

Keep prose fields only as display/render aids:

```rust
focus_summary
selection_rationale
```

### 9.5 Add registry source metadata

Every schema/alias should carry source:

```rust
Builtin
Project
User
Skill
Governance
SessionDraft
```

This prevents semantic laundering. A project/user schema does not become developer/system authority just because the broker selected it.

### 9.6 Seams that stay right

Still use:

```text
core/src/session/turn.rs::run_sampling_request
core/src/semantic_broker_runtime.rs
codex-semantic-broker crate
prompt-only overlay
feature gate default off
```

Still avoid:

```text
ContextManager::for_prompt
protocol variants
durable history mutation
context-maintenance source changes
subagent inheritance changes in v1
```

### 9.7 Slight seam reshaping

The core runtime adapter should build:

```rust
SchemaRegistrySnapshot
```

before building `BrokerInput`.

So v1 call-flow becomes:

```rust
let registry = semantic_broker_runtime::load_registry_snapshot(...)?;
let broker_input = semantic_broker_runtime::collect_broker_input(...)?;
let candidates = codex_semantic_broker::build_candidate_set(&broker_input, &registry);
let resolution = codex_semantic_broker::adjudicate_candidates(
    &broker_input,
    &candidates,
    &DeterministicAdjudicator::default(),
);
let packet = codex_semantic_broker::build_active_packet(&broker_input, &resolution);
```

### 9.8 What can stay simple in v1

V1 can still be narrow:

* static built-in registry only;
* no model call;
* no file-backed schema loading;
* no persistent aliases;
* no blocking clarify;
* no track durable state;
* no schema-authoring UX;
* no subagent inheritance changes.

But the structs and packet schema should already reflect those future concepts.

---

## 10. Long-term slice map

### Slice 1 — Deterministic overlay with future-shaped registry

Scope:

* `codex-semantic-broker` crate;
* static built-in `SchemaRegistrySnapshot`;
* deterministic `SchemaAdjudicator`;
* prompt-only active packet;
* feature flag default off;
* hook in `core/src/session/turn.rs::run_sampling_request`;
* no history mutation.

Success:

```text
v1 works as deterministic broker, but all public types already speak registry/candidate/resolution/packet.
```

### Slice 2 — Bounded candidate registry loading

Add repo-native schema loading:

```text
semantic-broker/schemas/builtin/*.schema.toml
.codex/semantic-broker/schemas/*.schema.toml
.codex/semantic-broker/aliases.toml
```

Core runtime loads schemas, validates them through the broker crate, and builds `SchemaRegistrySnapshot`.

No model adjudication yet.

### Slice 3 — Model-assisted bounded schema selection

Add optional feature-gated model-assisted adjudicator.

Rules:

* model receives only narrowed candidate set;
* model output is validated against `BrokerResolution` schema;
* model cannot invent schema IDs;
* if model proposes alias/novelty, it emits typed event only;
* deterministic fallback remains available.

### Slice 4 — Broker clarify UX

Add:

```rust
BrokerDecision::Clarify(ClarifyRequest)
```

Core behavior:

* broker can short-circuit main sampling;
* emit narrow assistant clarification;
* user reply feeds `BrokerClarificationOutcome`;
* resolution continues.

This is separate from model-side `request_user_input`.

### Slice 5 — Alias evolution and schema update events

Add persistence for:

```text
approved aliases
proposed aliases
rejected aliases
track-scoped aliases
repo-scoped aliases
```

Changes require explicit approval path. No silent alias mutation.

### Slice 6 — Interactive schema authoring mode

Add bounded schema-authoring workflow:

* schema-authoring operator family;
* guided required-field collection;
* meta-schema validation;
* draft/proposed/approved lifecycle;
* storage/versioning;
* alias-vs-new-schema decision support.

This is broker-adjacent, not a generic markdown generator.

### Slice 7 — Track-aware durable broker state

Add:

```rust
BrokerTrackState
```

Store:

* active schema;
* active operator;
* track-local aliases;
* open clarifications;
* dormant schema refs.

Use it for candidate narrowing and packet foregrounding.

### Slice 8 — Subagent scoped inheritance

Modify subagent spawn path:

```text
core/src/agent/control.rs
core/src/tools/handlers/multi_agents_v2/spawn.rs
```

Rules:

* parent active packet becomes background reference;
* child receives task-scoped packet;
* active schema inherited only if assignment requests it.

### Slice 9 — Broker-aware context maintenance

Only after durable broker state exists:

* teach context maintenance which broker artifacts are prompt-only, track-state, or durable;
* exclude stale active packets from thread-memory source;
* optionally include compacted track/schema state in thread memory or continuation bridge.

Do not start here. This is a later integration slice.

---

Net delta: the v1 hook and prompt-only packet remain correct, but v1 should no longer be described as “a deterministic resolver over six procedures.” It should be described as **the first adjudicator implementation over a typed schema registry snapshot**. That one change preserves the narrow first slice while making the long-term bounded semantic broker architecture first-class from day 1.
