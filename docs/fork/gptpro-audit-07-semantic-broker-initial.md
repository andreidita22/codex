## 1. Executive judgment

The broker should be wired as a **separate prompt-ingress pass that constructs a typed active packet immediately before the main sampling/orchestration path**, not as a rewrite of `ContextManager`, not inside compaction, and not as another chat agent.

Best shape:

```text
user turn accepted
  -> context updates / user prompt / skill-plugin injections recorded
  -> broker builds typed prompt overlay from current prompt candidates
  -> run_sampling_request receives prompt input + broker overlay
  -> broker overlay is injected into the model prompt
  -> main model/tool orchestrator runs
```

The important point is **prompt overlay, not durable history mutation** for slice 1. The active packet should be visible to the main orchestrator, but should not immediately become a durable conversation artifact that thread memory, continuation bridge, or compaction can ingest as if it were conversation content.

Recommended architecture:

```text
codex-semantic-broker        pure typed law / resolver / packet schema
core/src/semantic_broker.rs  runtime adapter over Session, TurnContext, history, tools
core/src/session/turn.rs     small insertion hook before build_prompt
```

This follows the same successful pattern as the two existing custom modules: semantic law lives in a domain module; `codex-core` remains an adapter/runtime executor. That is the accepted pattern for the observability extraction too: progress law is module-owned, while runtime waiting and tool planning remain outside the module.  

The vanilla post-122 `core/src/context/*` refactor is a useful substrate, but the fork is still on the older 122 shape. So the design should support both:

* **current fork:** render broker packet directly as a prompt-only `ResponseItem::Message`.
* **after absorbing current vanilla main:** represent the packet as a typed context fragment under `core/src/context/active_context_packet.rs`.

Do **not** put this in `ContextManager::for_prompt`. That would create hidden global law. Do **not** expand `codex_protocol::models::DeveloperInstructions`; the fork already has custom prompt-template logic there, and adding broker semantics to protocol would increase upstream contact in the wrong crate.

---

## 2. Observed current pipeline

### 2.1 Vanilla current main: observed

The current vanilla repo is post-122 and has moved context fragments into `core/src/context/`.

Observed files:

```text
core/src/context/fragment.rs
core/src/context/mod.rs
core/src/context/user_instructions.rs
core/src/context/environment_context.rs
core/src/context/permissions_instructions.rs
core/src/context/collaboration_mode_instructions.rs
core/src/context/available_skills_instructions.rs
core/src/context/skill_instructions.rs
core/src/context/plugin_instructions.rs
core/src/context/spawn_agent_instructions.rs
core/src/context/subagent_notification.rs
core/src/context/contextual_user_message.rs
```

`core/src/context/fragment.rs` defines the current typed fragment substrate:

```rust
pub trait ContextualUserFragment {
    const ROLE: &'static str;
    const START_MARKER: &'static str;
    const END_MARKER: &'static str;

    fn body(&self) -> String;
    fn matches_text(text: &str) -> bool;
    fn render(&self) -> String;
    fn into(self) -> ResponseItem;
}
```

Despite the name, the trait supports non-user roles. For example, `AvailableSkillsInstructions` uses role `"developer"`.

Observed context assembly path in vanilla:

```text
core/src/session/mod.rs::build_initial_context
core/src/context_manager/updates.rs::build_settings_update_items
```

`build_initial_context` collects developer sections and contextual user sections, then emits:

```text
developer message
contextual user message
optional guardian developer message
```

The current vanilla context fragment refactor is mostly a **rendering/recognition organization**. It does not yet attach first-class authority, salience, procedure IDs, track IDs, or memory/procedure distinction metadata.

### 2.2 Fork 122: observed

The fork does **not** yet have `core/src/context/*`. It still has older context modules:

```text
core/src/contextual_user_message.rs
core/src/environment_context.rs
core/src/context_manager/updates.rs
core/src/session/mod.rs
```

The fork has custom governance packet infrastructure:

```text
core/src/governance/packets.rs
core/src/governance/compiler.rs
core/src/governance/prompt_layers.rs
```

Observed governance types include:

```rust
pub enum NormativeForce {
    Hard,
    Default,
    Advisory,
    Factual,
}

pub enum PacketLayer {
    Constitution,
    Role,
    TaskCharter,
    TaskResidual,
    RuntimeFactStore,
    AdapterOverlay,
}
```

The fork’s `Session::build_initial_context` classifies current context sections into:

```text
constitutional_sections
role_sections
task_sections
runtime_sections
contextual_user_sections
developer_sections
```

Then it calls:

```rust
governance::prompt_layers::build_prompt_layering_section(...)
```

and inserts that into the developer message through:

```rust
context_manager::updates::insert_governance_prompt_layering_section(...)
```

This is adjacent to the broker, but it is not the broker. Current governance layering expresses **authority layering of known context sections**. The broker should express **turn-specific semantic ingress resolution**.

The fork also has custom context-maintenance law in:

```text
context-maintenance-policy/
core/src/context_maintenance_runtime.rs
core/src/compact.rs
core/src/compact_remote.rs
core/src/context_maintenance.rs
```

The policy crate owns artifact families such as:

```rust
ArtifactKind::ThreadMemory
ArtifactKind::ContinuationBridge
ArtifactKind::PruneManifest
ArtifactKind::CompactionMarker
```

and typed route/disposition law. The recent hardening explicitly kept context-maintenance scope narrow and avoided reopening governance semantics. 

The fork also has observability extracted into:

```text
agent-observability/
```

That crate should stay unrelated to the broker except for telemetry-style instrumentation. It owns progress/reducer/wait semantics, not semantic ingress.

### 2.3 User input ingress: observed

Current fork path:

```text
core/src/session/handlers.rs::user_input_or_turn_inner
```

It handles:

```rust
Op::UserTurn { ... }
Op::UserInput { ... }
```

Then:

```rust
sess.new_turn_with_sub_id(...)
sess.steer_input(...)
sess.spawn_task(..., RegularTask::new())
```

Regular task path:

```text
core/src/tasks/regular.rs::RegularTask::run
  -> core/src/session/turn.rs::run_turn
```

`RegularTask::run` emits `TurnStarted`, consumes startup prewarm, then loops through:

```rust
run_turn(sess, ctx, next_input, ...)
```

### 2.4 Main orchestrator path: observed

`core/src/session/turn.rs::run_turn` performs the operational turn setup:

1. pre-sampling compaction;
2. context update recording;
3. plugin/skill mention resolution;
4. user prompt hook execution;
5. user prompt recording;
6. additional context recording;
7. skill/plugin injection recording;
8. prompt input construction from history;
9. `run_sampling_request`.

The key observed line is:

```rust
let sampling_request_input: Vec<ResponseItem> = {
    sess.clone_history()
        .await
        .for_prompt(&turn_context.model_info.input_modalities)
};
```

Then:

```rust
run_sampling_request(..., sampling_request_input, ...)
```

Inside `run_sampling_request`:

```rust
let router = built_tools(...).await?;
let base_instructions = sess.get_base_instructions().await;

let mut initial_input = Some(input);
loop {
    let prompt_input = if let Some(input) = initial_input.take() {
        input
    } else {
        sess.clone_history().await.for_prompt(...)
    };

    let prompt = build_prompt(prompt_input, router.as_ref(), turn_context.as_ref(), base_instructions.clone());
    try_run_sampling_request(..., &prompt, ...).await
}
```

This is the most important insertion fact: **prompt input is materialized before `build_prompt`, and future loop iterations re-read history.**

### 2.5 Compaction / continuation / memory handling: observed

Context maintenance:

* `context-maintenance-policy/src/contracts.rs`
* `context-maintenance-policy/src/route_matrix.rs`
* `context-maintenance-policy/src/thread_memory.rs`
* `context-maintenance-policy/src/continuation_bridge/`
* `core/src/context_maintenance_runtime.rs`

Thread memory and continuation bridge are currently **developer-role tagged artifacts**. Source selection is policy-owned. For thread memory, developer messages are included unless they are recognized context-maintenance artifacts. Therefore, a broker packet recorded as an ordinary developer message would be eligible for future thread-memory ingestion unless explicitly excluded.

This is a decisive reason to avoid durable history insertion in slice 1.

Memory subsystem:

```text
core/src/memories/
```

Observed behavior:

* startup memory extraction/consolidation;
* `memory_summary.md`;
* `raw_memories.md`;
* memory extension roots;
* `build_memory_tool_developer_instructions(...)`.

The memory prompt is currently broad developer guidance. The broker should treat memory as advisory/factual candidate material. It must not silently promote memory into binding procedure.

### 2.6 Subagent inheritance path: observed

Fork and vanilla both have agent spawning through:

```text
core/src/agent/control.rs
```

Relevant observed functions:

```rust
spawn_agent_internal(...)
spawn_forked_thread(...)
keep_forked_rollout_item(...)
```

`spawn_forked_thread` forks rollout history. `keep_forked_rollout_item` preserves messages and some session metadata, but drops many tool/reasoning/image/ghost/compaction items.

In current fork, spawned-agent guidance is still a constant in:

```text
core/src/tools/handlers/multi_agents_v2/spawn.rs
SPAWN_AGENT_DEVELOPER_INSTRUCTIONS
```

In current vanilla main, it has moved into:

```text
core/src/context/spawn_agent_instructions.rs
SpawnAgentInstructions
```

This is a likely future upstream seam.

### 2.7 Inferred

The main operational orchestrator is not a single named “planner” module. In repo terms, it is the combination of:

```text
run_sampling_request
build_prompt
try_run_sampling_request
ToolCallRuntime
built_tools
```

So the broker should target the prompt boundary before `build_prompt`, not a nonexistent planner API.

### 2.8 Proposed

Add a broker prompt overlay between prompt-input materialization and `build_prompt`.

Do not insert it into raw history in slice 1.

---

## 3. Recommended broker architecture

### 3.1 Broker responsibilities

The broker should own:

* user-turn semantic typing;
* procedure/operator resolution;
* active/dormant artifact selection;
* authority-class labeling;
* salience ranking;
* active track selection;
* ambiguity detection;
* typed packet construction;
* packet rendering schema.

It should answer:

```text
What kind of turn is this?
What procedure/operator is probably intended?
Which context/memory/procedure/history artifacts are active?
Which artifacts are dormant?
What authority does each selected artifact have?
Should the main orchestrator proceed, or should the broker ask a narrow clarification?
```

### 3.2 Broker non-responsibilities

The broker should not:

* execute shell/tools;
* stream model output;
* own `Session`;
* own `TurnContext`;
* own `ContextManager`;
* rewrite compaction route law;
* generate thread memory or continuation bridge;
* own subagent lifecycle;
* register tools;
* become another conversational agent;
* merge memory, procedure, history, and instruction into one prose bucket.

### 3.3 Recommended crate/module split

Preferred long-term shape:

```text
semantic-broker/
  Cargo package: codex-semantic-broker
  Owns typed law:
    - BrokerInput
    - BrokerDecision
    - ActiveContextPacket
    - ProcedureRegistry
    - resolver thresholds
    - packet schema/rendering
    - tests

core/src/semantic_broker_runtime.rs
  Owns runtime collection:
    - Session/TurnContext access
    - history scanning
    - config/feature gate
    - prompt overlay injection
```

Dependencies for `codex-semantic-broker`:

```text
codex-protocol
serde
serde_json
thiserror
tracing optional
```

It should not depend on:

```text
codex-core
codex-tools
codex-config
tui/app-server
context-maintenance-policy
agent-observability
```

Core may depend on all of them and translate.

This is the same dependency geometry that worked for context maintenance and observability: typed semantic law in a crate; runtime adapters in core.

### 3.4 Input artifact classes

The broker should ingest typed **candidates**, not raw prose only.

Sketch:

```rust
pub struct BrokerInput {
    pub turn: UserTurnCandidate,
    pub session: SessionCandidate,
    pub authority_sources: Vec<AuthorityCandidate>,
    pub context_fragments: Vec<ContextFragmentCandidate>,
    pub maintenance_artifacts: Vec<MaintenanceArtifactCandidate>,
    pub memory_artifacts: Vec<MemoryArtifactCandidate>,
    pub procedure_registry: ProcedureRegistrySnapshot,
    pub track_candidates: Vec<TrackCandidate>,
    pub tool_surface: ToolSurfaceCandidate,
}
```

Concrete artifact classes to collect from current repo:

| Class                         | Current source                                                                        | Broker treatment                                                  |
| ----------------------------- | ------------------------------------------------------------------------------------- | ----------------------------------------------------------------- |
| Current user input            | `UserInput`, `UserMessageItem::new(&input)`                                           | primary turn intent source                                        |
| Base/system instructions      | `sess.get_base_instructions()`                                                        | highest authority reference; do not copy wholesale                |
| Developer instructions        | `turn_context.developer_instructions`, collaboration/personality/realtime/permissions | binding/default depending source                                  |
| Governance prompt layering    | `core/src/governance/*` output                                                        | authority map input, not a broker substitute                      |
| User instructions / AGENTS.md | `turn_context.user_instructions`, contextual fragments                                | user-level/task-level guidance                                    |
| Environment context           | `EnvironmentContext`                                                                  | runtime facts                                                     |
| Skills/plugins/apps           | skill/plugin injection items, tool summaries                                          | procedure/tool candidate hints                                    |
| Thread memory                 | `<thread_memory>` developer artifact                                                  | durable continuity, advisory/factual unless policy says otherwise |
| Continuation bridge           | `<continuation_bridge>` developer artifact                                            | turn-scoped continuity hint                                       |
| Prune manifest                | `<prune_manifest>`                                                                    | maintenance marker only                                           |
| Compact summaries             | compaction summary messages                                                           | historical summary, not procedure                                 |
| File-backed memory            | `core/src/memories`, memory prompt and summary paths                                  | advisory/factual memory candidates                                |
| Subagent inventory            | `AgentControl::format_environment_context_subagents` / environment context            | track/subtask candidates                                          |
| Session source                | `SessionSource`, `SubAgentSource`                                                     | inheritance and tool-surface context                              |
| Tool surface                  | `built_tools`, `ToolRouter::model_visible_specs`                                      | capability facts, not authority                                   |

### 3.5 Output packets

The broker should emit a canonical prompt overlay packet.

Minimal type sketch:

```rust
pub struct ActiveContextPacket {
    pub schema: &'static str,
    pub packet_id: String,
    pub turn_id: String,

    pub user_turn_kind: UserTurnKind,
    pub active_track: TrackRef,

    pub selected_procedure: Option<ResolvedProcedure>,
    pub procedure_ambiguity: Option<ProcedureAmbiguity>,

    pub authority_map: Vec<AuthorityBinding>,
    pub active_artifacts: Vec<ActiveArtifactRef>,
    pub dormant_artifacts: Vec<DormantArtifactRef>,

    pub constraints: Vec<ConstraintRef>,
    pub focus_summary: String,

    pub clarify: Option<ClarifyRequest>,
    pub witness: BrokerWitness,
}
```

Rendered as:

```text
<active_context_packet schema="codex.semantic_broker.active_packet.v1">
{
  ...
}
</active_context_packet>
```

Slice 1 should render this as a **prompt-only developer message**, not a durable history item.

### 3.6 Authority representation

Recommended broker authority type:

```rust
pub enum AuthorityClass {
    System,
    Developer,
    RuntimePolicy,
    AdapterOverlay,
    UserInstruction,
    UserTask,
    ProcedureDefinition,
    MemoryAdvisory,
    HistoricalFact,
    RuntimeFact,
}

pub enum NormativeForce {
    Binding,
    Default,
    Advisory,
    Factual,
}
```

The fork already has `core/src/governance/packets.rs::NormativeForce`. Do not directly merge broker and governance types in slice 1. Instead:

* broker may translate governance `NormativeForce` into broker-local authority classes;
* governance remains authority compiler;
* broker remains turn ingress compiler.

This avoids making governance prompt layering a second broker or making the broker a second governance compiler.

### 3.7 Salience and foregrounding

Use explicit salience and include mode:

```rust
pub enum IncludeMode {
    Inline,
    ReferenceOnly,
    Suppress,
}

pub struct ActiveArtifactRef {
    pub artifact_id: String,
    pub class: ArtifactClass,
    pub source: ArtifactSource,
    pub authority: AuthorityClass,
    pub force: NormativeForce,
    pub salience: f32,
    pub include_mode: IncludeMode,
    pub reason: String,
}
```

Memory advisory remains advisory even if highly salient.

Procedure selection can be binding only if the procedure registry itself is from a binding authority. Natural-language user selection of a procedure is a user-task directive, not developer law.

---

## 4. Precise insertion seams

### 4.1 Best slice-1 seam: `core/src/session/turn.rs::run_sampling_request`

**Observed location:**

```rust
async fn run_sampling_request(...)
```

Current flow:

```rust
let router = built_tools(...).await?;
let base_instructions = sess.get_base_instructions().await;

let mut initial_input = Some(input);
loop {
    let prompt_input = if let Some(input) = initial_input.take() {
        input
    } else {
        sess.clone_history().await.for_prompt(...)
    };

    let prompt = build_prompt(prompt_input, router.as_ref(), turn_context.as_ref(), base_instructions.clone());
}
```

**Recommended change:**

Build one broker overlay after router and base instructions are available, then apply it to every `prompt_input` before `build_prompt`.

Sketch:

```rust
let broker_overlay = semantic_broker_runtime::maybe_build_prompt_overlay(
    SemanticBrokerRuntimeInput {
        sess: sess.as_ref(),
        turn_context: turn_context.as_ref(),
        initial_prompt_input: &input,
        router: router.as_ref(),
        base_instructions: &base_instructions,
        explicitly_enabled_connectors,
        skills_outcome,
    },
).await?;

loop {
    let mut prompt_input = if let Some(input) = initial_input.take() {
        input
    } else {
        sess.clone_history()
            .await
            .for_prompt(&turn_context.model_info.input_modalities)
    };

    if let Some(overlay) = broker_overlay.as_ref() {
        overlay.apply_to_prompt_input(&mut prompt_input);
    }

    let prompt = build_prompt(prompt_input, router.as_ref(), turn_context.as_ref(), base_instructions.clone());
}
```

**Why this seam is good:**

* exact prompt input is available;
* router/tool surface is available;
* base instructions are available;
* no history mutation is required;
* packet remains active across tool-loop sampling iterations;
* it does not affect compaction, thread memory, or rollout persistence in slice 1.

**Risk:** medium-low. `turn.rs` is still hot, but the 122 split localized this seam. The change can be a small hook.

**Slice suitability:** slice 1.

---

### 4.2 Secondary seam: `core/src/session/turn.rs::run_turn`

**Observed location:**

After:

```rust
record_context_updates_and_set_reference_context_item
record_user_prompt_and_emit_turn_item
record_additional_contexts
record_conversation_items(skill_items)
record_conversation_items(plugin_items)
```

Before:

```rust
let sampling_request_input = sess.clone_history().await.for_prompt(...)
run_sampling_request(...)
```

**Use:**

Collect turn-level broker candidate facts here if needed.

**Why good:**

* user prompt and contextual injections have been recorded;
* skill/plugin mention resolution is complete;
* current turn context is final.

**Why not primary:**

If the packet is recorded into history here, it contaminates thread memory/continuation bridge source selection unless additional exclusions are added. For slice 1, avoid history insertion.

**Risk:** medium.

**Slice suitability:** candidate collection only; not packet insertion.

---

### 4.3 Bad seam: `ContextManager::for_prompt`

**Observed file:**

```text
core/src/context_manager/history.rs
```

**Why bad:**

`for_prompt` is a generic normalization/filtering function. Adding broker logic here would affect every prompt retrieval path and hide semantic ingress law inside history plumbing.

**Slice suitability:** never, unless upstream later makes it an explicit prompt-planning extension point.

---

### 4.4 Current fork context assembly seam: `Session::build_initial_context`

**Observed file:**

```text
core/src/session/mod.rs
```

**Use:**

Only for collecting context candidate metadata or, later, static fragment descriptors.

**Why not broker insertion:**

At this point the broker does not yet have the actual user turn intent. It can see context, not ingress semantics.

**Risk:** high if used for broker decision logic, because this file already carries fork governance layering changes.

**Slice suitability:** no for slice 1 decisioning.

---

### 4.5 Current vanilla main context fragment seam: `core/src/context/*`

**Observed files:**

```text
core/src/context/fragment.rs
core/src/context/mod.rs
```

**Recommended future use:**

After absorbing current vanilla main, add:

```text
core/src/context/active_context_packet.rs
```

with:

```rust
pub(crate) struct ActiveContextPacketFragment {
    packet_json: String,
}

impl ContextualUserFragment for ActiveContextPacketFragment {
    const ROLE: &'static str = "developer";
    const START_MARKER: &'static str = "<active_context_packet>";
    const END_MARKER: &'static str = "</active_context_packet>";

    fn body(&self) -> String {
        format!("\n{}\n", self.packet_json)
    }
}
```

**Why good:**

* aligns with upstream context organization;
* one renderer/marker owner;
* packet is recognizable by future filters.

**Risk:** low after the context refactor is ingested; not available in current fork yet.

**Slice suitability:** later, unless you first absorb the context refactor.

---

### 4.6 Subagent inheritance seam: `core/src/agent/control.rs`

**Observed functions:**

```rust
spawn_agent_internal(...)
spawn_forked_thread(...)
keep_forked_rollout_item(...)
```

**Use:**

Later slice for scoped inheritance.

**Recommended behavior:**

Parent active packet should not be inherited as active law by default. It should be downgraded to parent-background context unless the spawn assignment explicitly foregrounds it.

**Risk:** medium-high. This touches multi-agent lifecycle and forked history.

**Slice suitability:** not slice 1.

---

### 4.7 Context-maintenance seam

**Observed files:**

```text
context-maintenance-policy/src/thread_memory.rs
context-maintenance-policy/src/continuation_bridge/
context-maintenance-policy/src/history_shape.rs
core/src/compact.rs
core/src/compact_remote.rs
```

**Use:**

Later slice for broker-aware compaction and stale packet exclusion.

**Important rule:**

If active packets ever become durable history items, context-maintenance source selection must explicitly classify them. Otherwise thread memory can ingest broker packets as ordinary developer context.

**Slice suitability:** avoid by prompt-only injection in slice 1.

---

## 5. Procedure-resolution design

### 5.1 First-pass procedure registry

Use a typed procedure/operator registry, not prose prompts.

Sketch:

```rust
pub struct OperatorSchema {
    pub id: ProcedureId,
    pub operator: OperatorKind,
    pub aliases: &'static [&'static str],
    pub predicates: &'static [&'static str],
    pub expected_artifacts: &'static [ArtifactClass],
    pub authority_required: ProcedureAuthorityRequirement,
    pub clarify_policy: ClarifyPolicy,
}

pub enum OperatorKind {
    Assess,
    Audit,
    Review,
    Verify,
    Judge,
    Compare,
    Implement,
    Debug,
    Plan,
    Summarize,
}

pub struct ResolvedProcedure {
    pub id: ProcedureId,
    pub operator: OperatorKind,
    pub confidence: f32,
    pub source: ProcedureResolutionSource,
    pub matched_terms: Vec<String>,
    pub expected_inputs: Vec<ArtifactClass>,
}
```

Initial built-ins should be small and specific:

```text
audit.architecture
review.implementation_spec
verify.claims_against_repo
compare.fork_to_vanilla
implement.patch
debug.failure
summarize.thread
plan.pr_sequence
```

Do not attempt a full procedure ontology in slice 1.

### 5.2 Turn kind detection

Typed turn kinds:

```rust
pub enum UserTurnKind {
    TaskRequest,
    InstructionLike,
    Corrective,
    Procedural,
    Verification,
    Conversational,
    Continuation,
    Ambiguous,
}
```

Detection inputs:

* current `UserInput::Text`;
* visible attachments/files if present;
* current skill/plugin mentions;
* recent user turn if this looks like a correction;
* active track from previous packet later.

Examples:

| User text pattern                        | Turn kind           |
| ---------------------------------------- | ------------------- |
| “review this implementation spec”        | Procedural / Review |
| “audit the fork against vanilla”         | Procedural / Audit  |
| “verify whether this broke 121 behavior” | Verification        |
| “no, that’s not what I meant”            | Corrective          |
| “continue”                               | Continuation        |
| “thanks”                                 | Conversational      |

### 5.3 Scoring

Use deterministic scoring first.

Example scoring:

```text
explicit operator verb at start            +0.35
operator alias anywhere                    +0.20
artifact class match                       +0.25
domain target match                        +0.20
explicit procedure ID                      +0.45
prior active track continuity              +0.10
conflicting operator terms                 -0.20
missing expected artifact                  -0.25
```

Thresholds:

```text
>= 0.75
  auto-resolve procedure

0.55..0.75
  tentative resolve; mark clarify_recommended
  if top two candidates within 0.12, mark ambiguous

< 0.55
  no procedure selected
  if procedural language detected, clarify_required
```

### 5.4 Clarify behavior

First slice:

* no blocking clarification;
* packet can carry:

```rust
ClarifyRequest {
    required: bool,
    question: String,
    options: Vec<ClarifyOption>,
}
```

The main orchestrator receives this and may ask the question.

Future slice:

* broker can return:

```rust
BrokerDecision::Clarify(ClarifyRequest)
```

and `run_turn` can short-circuit main sampling by emitting a normal assistant clarification message through existing event/history paths.

Do not use model-side `request_user_input` as the broker’s first clarification mechanism. That tool belongs to model execution; the broker is upstream of model execution.

### 5.5 No semantic laundering rules

Required resolver rule:

```text
Memory artifacts may increase relevance score.
Memory artifacts may not create binding procedure or developer authority.
```

Example:

If `memory_summary.md` says “the user likes architecture audits,” the broker may select `audit.architecture` with memory as a weak prior. It must not emit “binding instruction: always audit architecture.”

If AGENTS.md says “use procedure X,” it should be tagged as user/project instruction, not system/developer law.

---

## 6. Track isolation / foregrounding design

### 6.1 Current repo state

Observed current state is essentially single linear history plus:

* subagent inventory;
* forked rollout history;
* compacted summaries;
* thread memory;
* continuation bridge;
* pending input;
* additional hook context;
* memory files.

There is no first-class track object today.

### 6.2 Broker track types

Introduce broker-local track concepts:

```rust
pub struct TrackRef {
    pub id: String,
    pub label: String,
    pub status: TrackStatus,
    pub source: TrackSource,
    pub confidence: f32,
}

pub enum TrackStatus {
    Active,
    Dormant,
    Closed,
}

pub enum TrackSource {
    CurrentUserTurn,
    ContinuationBridge,
    ThreadMemory,
    SubAgent,
    Procedure,
    Manual,
}
```

### 6.3 First slice

Do not try to suppress dormant tracks yet.

Slice 1 should:

* identify one active track for the current turn;
* list dormant candidates only as references;
* not remove history items;
* not alter compaction;
* not alter subagent history inheritance.

Packet example:

```json
{
  "active_track": {
    "id": "turn:abc123",
    "label": "review implementation spec",
    "source": "current_user_turn",
    "confidence": 0.86
  },
  "dormant_tracks": [
    {
      "id": "subagent:lint-runner",
      "label": "previous lint investigation",
      "include_mode": "reference_only"
    }
  ]
}
```

### 6.4 Later suppression

Later, the broker can emit:

```rust
PromptAssemblyPlan {
    foreground: Vec<ArtifactRef>,
    reference_only: Vec<ArtifactRef>,
    suppress: Vec<ArtifactRef>,
}
```

But this should be applied at the prompt overlay/planning layer, not by mutating `ContextManager::for_prompt`.

### 6.5 Compaction / rehydration

Broker-aware compaction should come after active packet semantics are proven.

Possible later rule:

* latest active packet can be retained as broker state;
* stale active packets are excluded from thread-memory source;
* continuation bridge may include current active track ID;
* thread memory may include durable track summaries, but not raw broker packets.

This likely requires a new broker artifact class or explicit source-selection predicates in context maintenance. Do not start there.

### 6.6 Subagent scoped inheritance

Later subagent behavior should be:

```text
parent active packet
  -> downgraded to parent_context reference
  -> only assignment-relevant artifacts foregrounded
  -> child gets its own active packet for assigned task
```

This belongs near:

```text
core/src/agent/control.rs::spawn_agent_internal
core/src/agent/control.rs::spawn_forked_thread
core/src/tools/handlers/multi_agents_v2/spawn.rs
```

After vanilla context refactor is absorbed, use:

```text
core/src/context/spawn_agent_instructions.rs
```

as the typed fragment seam.

---

## 7. Interaction with current vanilla context-fragment refactors

### 7.1 What vanilla now gives you

Current vanilla main’s `core/src/context/*` organization is a good substrate because it gives:

* typed fragment structs;
* marker ownership;
* role ownership;
* fragment registration and recognition;
* central context module.

This directly helps with broker packet rendering and recognition.

### 7.2 What vanilla still does not provide

The current context fragment trait does not encode:

* authority class;
* salience;
* source identity;
* procedure ID;
* track ID;
* lifecycle/durability;
* active vs dormant status;
* memory vs procedure vs instruction distinction.

So the broker should not wait for vanilla to solve this. It should wrap fragments into broker-local candidate descriptors:

```rust
pub struct ContextFragmentCandidate {
    pub fragment_id: String,
    pub marker: Option<String>,
    pub role: String,
    pub class: ContextFragmentClass,
    pub authority: AuthorityClass,
    pub force: NormativeForce,
    pub source: ArtifactSource,
}
```

### 7.3 How to leverage vanilla without diverging

After absorbing vanilla context refactor:

* add `ActiveContextPacketFragment` under `core/src/context/`;
* register it in `contextual_user_message.rs` only if it ever becomes durable history;
* keep broker semantic types outside `core/src/context`;
* keep `core/src/context` as rendering/recognition substrate only.

Do not modify `ContextualUserFragment` to carry authority metadata in the first pass. That would be a broad upstream divergence.

---

## 8. First slice plan

### 8.1 Scope

Implement a **default-off deterministic broker prompt overlay**.

It should:

* classify the current user turn;
* resolve a small built-in procedure/operator set;
* gather candidate metadata from the current prompt input;
* emit one prompt-only `ActiveContextPacket`;
* not mutate history;
* not suppress dormant tracks;
* not ask blocking clarifications;
* not perform an extra model call.

### 8.2 Feature gate

Add a new feature flag:

```rust
Feature::SemanticBroker
```

Suggested key:

```text
semantic_broker
```

Stage:

```rust
Stage::UnderDevelopment
```

Default:

```rust
false
```

File:

```text
features/src/lib.rs
```

### 8.3 New crate

Add:

```text
semantic-broker/
```

Cargo package:

```text
codex-semantic-broker
```

Files:

```text
semantic-broker/src/lib.rs
semantic-broker/src/types.rs
semantic-broker/src/procedures.rs
semantic-broker/src/resolver.rs
semantic-broker/src/render.rs
semantic-broker/src/tests.rs
```

Initial public API:

```rust
pub struct BrokerInput { ... }
pub struct BrokerDecision { ... }
pub struct ActiveContextPacket { ... }

pub fn resolve_broker_packet(input: BrokerInput) -> BrokerDecision;
pub fn render_active_context_packet(packet: &ActiveContextPacket) -> String;
pub fn active_context_packet_response_item(packet: &ActiveContextPacket) -> ResponseItem;
```

The response item constructor can live in the crate because it only depends on `codex-protocol::models::ResponseItem`.

### 8.4 Core adapter

Add:

```text
core/src/semantic_broker_runtime.rs
```

Responsibilities:

* check feature flag;
* collect prompt input candidates;
* scan for context-maintenance artifact tags;
* extract current user turn text;
* collect session source and governance path;
* collect tool names/capability summaries from `ToolRouter`;
* build `codex_semantic_broker::BrokerInput`;
* return a prompt overlay.

Sketch:

```rust
pub(crate) struct BrokerPromptOverlay {
    packet_item: ResponseItem,
}

impl BrokerPromptOverlay {
    pub(crate) fn apply_to_prompt_input(&self, prompt_input: &mut Vec<ResponseItem>) {
        prompt_input.push(self.packet_item.clone());
    }
}
```

Runtime function:

```rust
pub(crate) async fn maybe_build_prompt_overlay(
    input: SemanticBrokerRuntimeInput<'_>,
) -> CodexResult<Option<BrokerPromptOverlay>>;
```

### 8.5 Hook location

Modify:

```text
core/src/session/turn.rs::run_sampling_request
```

Add after:

```rust
let router = built_tools(...).await?;
let base_instructions = sess.get_base_instructions().await;
```

Before the loop:

```rust
let broker_overlay =
    crate::semantic_broker_runtime::maybe_build_prompt_overlay(...).await?;
```

Then in the loop:

```rust
if let Some(overlay) = broker_overlay.as_ref() {
    overlay.apply_to_prompt_input(&mut prompt_input);
}
```

Do not record the packet through:

```rust
sess.record_conversation_items(...)
```

in slice 1.

### 8.6 Packet placement

For slice 1, append as the final developer message in prompt input:

```text
history/context/user/tool items
developer: <active_context_packet>...</active_context_packet>
```

This is simple, model-visible, and avoids splitting tool call/output pairs because `for_prompt` already normalized history before overlay insertion.

### 8.7 Initial procedure registry

Start with six operators:

```text
review.implementation_spec
audit.architecture
verify.repo_claim
compare.vanilla_fork
implement.patch
summarize.thread
```

Each schema should define:

```rust
id
operator kind
aliases
expected artifact classes
confidence thresholds
clarify policy
```

### 8.8 Candidate extraction

Slice 1 candidate extraction should be shallow:

* current user text;
* current prompt input item roles and fragment markers;
* latest context-maintenance artifact kinds present;
* memory tool prompt present/absent;
* session source;
* governance path variant;
* tool names visible to model;
* skill/plugin mention metadata already computed in `run_turn`.

Do not read arbitrary memory files in slice 1. The memory prompt already handles file-backed memory; the broker should only note memory availability.

### 8.9 Logging / observability

Use tracing, not agent-observability.

Suggested fields:

```text
semantic_broker.enabled
semantic_broker.packet_id
semantic_broker.turn_kind
semantic_broker.selected_procedure
semantic_broker.confidence
semantic_broker.clarify_required
semantic_broker.active_artifact_count
semantic_broker.dormant_artifact_count
```

Do not expose broker packet internals in user-facing events by default.

### 8.10 Tests

Crate tests:

```text
semantic-broker/src/resolver.rs
```

Cover:

* “review this implementation spec” → `review.implementation_spec`;
* “compare vanilla and our fork” → `compare.vanilla_fork`;
* “verify whether this broke 121 behavior” → `verify.repo_claim`;
* ambiguous “assess this” → tentative or clarify;
* memory candidate cannot become binding;
* developer/system source outranks memory.

Core tests:

* feature off produces no overlay;
* feature on appends exactly one active packet to prompt input;
* packet is not recorded in `ContextManager`;
* packet appears in first and follow-up sampling prompts within a tool loop;
* no context-maintenance artifact generation changes.

### 8.11 Success criteria

Slice 1 is successful when:

* feature off is behaviorally unchanged;
* feature on emits a typed active packet into prompt input;
* packet does not persist into history;
* procedure resolution is deterministic and testable;
* no compaction/thread-memory/continuation behavior changes;
* no protocol schema changes are required.

---

## 9. Future slices

### Slice 2 — Model-assisted procedure resolver

Add optional model-assisted broker resolution only when deterministic confidence is insufficient.

Shape:

```text
codex-semantic-broker owns JSON schema + parser
core adapter performs model call
broker crate validates result
```

Do not let the model invent procedure IDs. It may choose from registry candidates or request clarification.

### Slice 3 — Broker-driven clarify UX

Add:

```rust
BrokerDecision::Clarify(ClarifyRequest)
```

Core behavior:

* short-circuit main sampling;
* emit a normal assistant clarification message;
* record it in history;
* wait for user reply.

This should happen before main orchestrator sampling, not through model-side `request_user_input`.

### Slice 4 — Durable track state

Introduce:

```rust
BrokerTrackState
```

Track:

* active track ID;
* dormant track IDs;
* last selected procedure;
* open obligations;
* foregrounded artifacts.

Do not put this into protocol first. Consider a tagged broker state artifact or local rollout metadata, then harden after behavior is proven.

### Slice 5 — Subagent scoped inheritance

Modify subagent spawn path:

```text
core/src/agent/control.rs
core/src/tools/handlers/multi_agents_v2/spawn.rs
```

Rules:

* parent packet becomes parent-background reference;
* child gets its own active packet;
* assignment procedure can be inherited if explicitly relevant;
* dormant parent tracks are suppressed.

After vanilla context refactor is absorbed, use `core/src/context/spawn_agent_instructions.rs` rather than maintaining raw string constants.

### Slice 6 — Broker-aware context maintenance

Add explicit policy for active packet lifetime.

Options:

```rust
BrokerPacketLifetime::PromptOnly
BrokerPacketLifetime::DurableUntilNextTurn
BrokerPacketLifetime::TrackStateOnly
```

Then update context-maintenance source selection so stale broker packets are not ingested into thread memory as facts.

This is where `context-maintenance-policy` may need a new artifact class or a generic “control artifact exclusion” hook. Do not do this before slice 1 proves the packet useful.

### Slice 7 — Memory relevance mediation

Move from broad memory prompt presence to broker-resolved memory references:

```text
memory_summary.md available
specific memory file relevant
memory extension relevant
memory advisory only
```

The broker should emit memory refs, not paste full memory bodies.

---

## 10. Upstream compatibility strategy

### 10.1 Prefer prompt overlay over history mutation

The most fork-safe first slice is prompt-only overlay injection.

Why:

* no `ContextManager` behavior changes;
* no rollout format changes;
* no compaction changes;
* no thread-memory source-selection change required;
* easy feature-off behavior;
* easier upstream rebase.

### 10.2 Avoid protocol changes

Do not add broker packet variants to:

```text
protocol/src/models.rs
protocol/src/protocol.rs
```

in early slices.

Use existing:

```rust
ResponseItem::Message {
    role: "developer",
    content: vec![ContentItem::InputText { ... }]
}
```

The fork already has custom `DeveloperInstructions` in `protocol/src/models.rs`; do not deepen that divergence.

### 10.3 Avoid `ContextManager::for_prompt`

Do not put broker logic in:

```text
core/src/context_manager/history.rs::for_prompt
```

That function is a generic normalization boundary. Broker law there would be invisible and high-churn.

### 10.4 Use vanilla context fragments after ingestion

Once the fork absorbs current vanilla main’s `core/src/context/*` refactor:

* move packet rendering into `core/src/context/active_context_packet.rs`;
* use `ContextualUserFragment` for marker/role rendering;
* keep broker semantic types in `codex-semantic-broker`.

This reduces long-term fork-specific rendering code.

### 10.5 Keep module boundaries narrow

Do not make a generic “fork broker hub” that owns context maintenance, observability, governance, memory, and tools. That would recreate the hidden-law problem the prior refactors avoided.

The existing custom module boundaries should remain:

| Feature                                | Owner                                           |
| -------------------------------------- | ----------------------------------------------- |
| Context-maintenance route/artifact law | `codex-context-maintenance-policy`              |
| Agent progress/wait/tool-contract law  | `codex-agent-observability`                     |
| Tool-surface policy                    | `codex-tools`                                   |
| Governance prompt layering             | `core/src/governance` for now                   |
| Semantic ingress broker                | proposed `codex-semantic-broker` + core adapter |

The observability docs explicitly accepted this domain-module pattern rather than a generic multi-agent or collaboration crate. 

### 10.6 Watch upstream seams

For future vanilla releases, broker-sensitive watch files are:

```text
core/src/session/turn.rs
core/src/session/mod.rs
core/src/context/*
core/src/context_manager/history.rs
core/src/context_manager/updates.rs
core/src/agent/control.rs
core/src/tools/*
protocol/src/models.rs
protocol/src/user_input.rs
```

Specific functions/concepts to watch:

| Upstream area                                  | Why it matters                           |
| ---------------------------------------------- | ---------------------------------------- |
| `run_sampling_request`                         | broker prompt overlay seam               |
| `build_prompt`                                 | prompt packet placement                  |
| `build_initial_context`                        | candidate fragments and authority inputs |
| `build_settings_update_items`                  | context diff candidates                  |
| `ContextualUserFragment`                       | future packet renderer                   |
| `ContextManager::for_prompt`                   | prompt input shape                       |
| `spawn_agent_internal` / `spawn_forked_thread` | scoped inheritance                       |
| `ToolRouter::model_visible_specs`              | tool-surface candidate summary           |
| `UserInput` shape                              | turn intent extraction                   |
| `ResponseItem` / `ContentItem` shape           | packet rendering and artifact scanning   |

### 10.7 Rebase pain to avoid

High-pain choices:

* modifying protocol to add broker packet variants;
* changing `ContextManager` semantics;
* recording active packets in history before compaction policy understands them;
* making broker depend on `codex-core`;
* putting procedure resolution into `session/mod.rs`;
* expanding governance prompt layering into turn-level procedure resolution;
* making memory prompts binding;
* hardcoding broker strings in several crates.

Low-pain choices:

* pure broker crate;
* small runtime adapter;
* feature flag default off;
* prompt-only overlay;
* typed packet schema;
* context-fragment renderer after vanilla context refactor is absorbed;
* deterministic resolver tests in the broker crate.

---

## Final recommended geometry

The broker should be a **typed ingress compiler**, not a durable memory artifact and not a new orchestrator.

First landing geometry:

```text
codex-semantic-broker
  -> typed resolver + packet schema

core/src/semantic_broker_runtime.rs
  -> collects Session/TurnContext/prompt/tool candidates

core/src/session/turn.rs::run_sampling_request
  -> builds broker overlay once
  -> injects overlay into every prompt input before build_prompt
```

The packet should be prompt-only at first:

```text
not history
not compaction input
not thread-memory source
not continuation bridge source
not protocol variant
```

That gives you the intended upstream semantic membrane with minimal vanilla divergence. It also preserves room for the later, harder work: broker-driven clarification, durable track state, subagent inheritance, and broker-aware compaction.
