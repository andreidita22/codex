## 1. Executive judgment

**Observed judgment:** the new `context-maintenance-policy` crate is directionally correct, but the boundary is not yet semantically sealed. It has successfully removed the central route matrix and most artifact prompt/schema ownership from `codex-core`, but several laws are still split between the policy crate and hot core files.

**Fundamentally correct:** yes, with reservations. The current split is strongest where the policy crate owns:

* `MaintenanceAction`, `MaintenanceTiming`, `PolicyEngine`, artifact kinds/lifetimes, and route planning in `context-maintenance-policy/src/contracts.rs` and `route_matrix.rs`.
* Continuation bridge prompt/schema/parsing/response-item emission in `context-maintenance-policy/src/continuation_bridge`.
* Thread-memory prompt/schema/parsing/response-item emission in `context-maintenance-policy/src/thread_memory.rs`.
* Generic history-shaping algorithms in `context-maintenance-policy/src/history_shape.rs`, with core supplying runtime predicates.

**Already strong:**

* No TUI/app-server/runtime-facing crate directly depends on `codex-context-maintenance-policy`. The observed direct users are `core` and tests.
* `core/src/context_maintenance_runtime.rs` is a real adapter layer for `CompactionEngine`, governance config, and route planning.
* The route matrix fails closed for unsupported routes.
* The policy crate owns the artifact tags/codecs for `thread_memory`, `continuation_bridge`, and `prune_manifest`, rather than scattering these tags across core.
* The history-shaping functions take closures for “real user”, “summary-like user”, and “raw conversation” detection. That keeps `event_mapping` and runtime-specific `TurnItem` parsing out of the policy crate.

**Structurally weak:**

* The policy crate still imports `codex_config::config_toml::GovernancePathVariant`. That is a config/TOML schema leak into the policy layer.
* `MaintenancePolicyPlan` declares `ArtifactLifetime`, but `RuntimeMaintenancePlan` discards lifetimes and stores only `Vec<ArtifactKind>`. Lifetime is currently mostly documentary.
* `drop_prior_artifact_kinds` is only executed by `/refresh` and `/prune`; compact paths do not uniformly apply it.
* `LegacyCompactionMarkerPolicy::Strip` is declared by the route matrix for refresh/prune/local compact paths, but refresh/prune do not actually strip `ResponseItem::Compaction`.
* Artifact requiredness is hidden in core: thread memory failures are fatal, continuation bridge failures are warning-only/best-effort. The policy plan does not encode that.
* Timing law is partially inferred from `InitialContextInjection` in core via `compact_timing_from_initial_context_injection`, which reverses the intended ownership: timing should be an input to policy; injection should be an output.
* The continuation-bridge prompt/schema lives in policy, but the `<continuation_bridge_subagents>` supplemental block format lives in `core/src/continuation_bridge/subagent_context.rs`.
* `thread_memory.rs` contains protocol/runtime pairing law for function/tool call output cleanup. That is not pure policy law.

## 2. Observed boundary map

### Policy crate outbound dependencies

**Observed: `context-maintenance-policy/Cargo.toml` depends on `codex-config`, `codex-protocol`, `serde`, `serde_json`, `thiserror`, and `tracing`.**

**Good contact:**

* `codex-protocol::models::ResponseItem` and `ContentItem` are used for artifact codecs and history transforms. This is acceptable for now because artifacts are physically represented as `ResponseItem::Message` developer blocks. Creating a mirror AST would be fake abstraction at this stage.

**Risky contact:**

* `codex_config::config_toml::GovernancePathVariant` is imported directly in `contracts.rs` and `route_matrix.rs`. This makes the policy crate depend on the TOML-facing config schema. The policy crate should receive a policy-owned governance axis, not a deserialization type.

### Core-to-policy contact points

**Observed: `core/src/context_maintenance_runtime.rs`**

Good adapter role:

* Maps core `CompactionEngine` to policy `PolicyEngine`.
* Calls `plan_route`.
* Converts `MaintenancePolicyPlan` to `RuntimeMaintenancePlan`.

Risky details:

* Maps core governance enum back into `codex_config::config_toml::GovernancePathVariant`, because the policy crate accepts the TOML type.
* Drops `ArtifactLifetime` when converting `ArtifactRequest` to `ArtifactKind`.
* Converts `ContextInjectionPolicy::BeforeLastRealUserOrSummary` into `InitialContextInjection::BeforeLastUserMessage`, losing the “or summary” semantics in the core-facing name.
* Derives `MaintenanceTiming` from `InitialContextInjection`. That couples timing law to injection placement.
* Produces duplicate unsupported-route wording: core prepends “Unsupported context-maintenance route” to an error that already says “unsupported context-maintenance route”.

**Observed: `core/src/compact.rs`**

Good contact:

* Uses policy-owned insertion/retention helpers through core predicates.
* Uses route plan to decide whether to generate `ThreadMemory` or `ContinuationBridge`.

Risky details:

* `COMPACT_RAW_CONVERSATION_WINDOW_MESSAGES = 5` is core-owned, but it drives policy-level retention behavior.
* Local compact does not explicitly execute `drop_prior_artifact_kinds`; it relies on rebuilding replacement history from selected user messages and summary.
* Thread memory generation is fatal if requested, but continuation bridge generation is best-effort. That requiredness is core-only law.
* `content_items_to_text` is duplicated in core even though policy exports the same helper.

**Observed: `core/src/compact_remote.rs`**

Good contact:

* Uses `shape_remote_compacted_history` with core-supplied predicates. This is the right adapter pattern because `event_mapping::parse_turn_item` remains core-owned.
* Passes policy `ContextInjectionPolicy` into history shaping.

Risky details:

* `should_keep_compacted_history_item` receives a reduced `preserve_legacy_compaction_marker: bool`, not the policy enum. The distinction between `Preserve` and `PreserveForUpstreamCompatibility` is lost at the executor boundary.
* Legacy compaction marker handling is only enforced here, not consistently across refresh/prune.
* Remote compact also does not use `drop_prior_artifact_kinds` directly; it relies on “drop all developer messages” plus explicit authoritative re-insertion.
* Uses the same core-owned raw-retention limit.

**Observed: `core/src/context_maintenance.rs`**

Good contact:

* `/refresh` and `/prune` call `runtime_plan_for_turn_boundary_maintenance`.
* Uses policy artifact codecs: `prune_superseded_artifacts`, `remove_artifact_kind`, `build_prune_manifest_item`, `tagged_artifact_kind`.

Risky details:

* This is the only place that actually iterates `runtime_plan.drop_prior_artifact_kinds()`.
* Refresh/prune do not apply `LegacyCompactionMarkerPolicy::Strip`.
* `retain_latest_summary_message` and `is_compaction_summary_message` are core-local carry/drop law.
* Retention after refresh/prune is driven by “has thread memory” plus the core constant `COMPACT_RAW_CONVERSATION_WINDOW_MESSAGES`.

**Observed: `core/src/continuation_bridge.rs`**

Good contact:

* Maps core `ContinuationBridgeVariant` to policy `BridgeVariant`.
* Uses policy-owned prompt, schema, parser, and response-item builder.
* Keeps model selection, streaming, telemetry, and session access in core.

Risky detail:

* Supplemental subagent context format is core-owned while the policy prompt explicitly names `<continuation_bridge_subagents>`.

**Observed: `core/src/governance/thread_memory.rs`**

Good contact:

* Core owns model request setup and streaming.
* Policy owns prompt construction, source selection, trimming, schema, parsing, and response item construction.

Risky details:

* Passes `SUMMARY_PREFIX.trim_end()` into policy. This is acceptable as an adapter, but still stringly.
* Uses policy `content_items_to_text`; meanwhile `compact.rs` has a duplicate.

### Config/protocol/app-server/runtime-facing crates

**Observed: config**

* `config/src/config_toml.rs` defines TOML-facing `ContinuationBridgeVariant`, `GovernancePathVariant`, `CompactionEngine`, and `ContextMaintenanceReasoningEffort`.
* `core/src/config/mod.rs` mirrors these into core runtime config types.

This is normal. The bad part is not the config/core mapping; the bad part is policy depending on the TOML enum.

**Observed: protocol/app-server**

* `protocol/src/protocol.rs` exposes `Op::RefreshContext`, `Op::PruneContext`, `Op::SetThreadMemoryMode`, etc.
* `app-server-protocol` mirrors `ThreadMemoryMode`.
* These crates do not import the policy crate.

This is good. Do not push policy semantics into protocol/app-server.

**Observed: state memory mode**

* State DB has `memory_mode` strings such as `enabled`, `disabled`, and `polluted`.
* This is related to global/thread memory eligibility, not the same semantic object as the context-maintenance `ThreadMemory` artifact.

Do not merge these concepts just because they share the phrase “thread memory”.

## 3. Leak taxonomy

| Leak                                                                  |                         Type | Observed location                                                    | Why it matters                                                                                                                                                                |
| --------------------------------------------------------------------- | ---------------------------: | -------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `codex_config::config_toml::GovernancePathVariant` imported by policy |      Config/TOML schema leak | `context-maintenance-policy/src/contracts.rs`, `route_matrix.rs`     | The policy crate should not depend on config deserialization shape. Any config schema churn now touches policy.                                                               |
| `ArtifactLifetime` discarded by core                                  |                Contract loss | `RuntimeMaintenancePlan` stores only `Vec<ArtifactKind>`             | `TurnScoped`, `DurableAcrossTurns`, and `MarkerOnly` are declared but not executable contract. Future lifetimes will reopen core.                                             |
| Artifact requiredness hidden in core                                  |            Hidden policy law | `compact.rs`, `compact_remote.rs`, `context_maintenance.rs`          | Thread memory is required/fatal; continuation bridge is best-effort. The route plan does not encode this, so artifact-family meaning is split.                                |
| `drop_prior_artifact_kinds` only applied in refresh/prune             |           Split executor law | `core/src/context_maintenance.rs` only                               | Compact routes declare drop behavior, but compact paths do not execute that field uniformly. Current behavior works by incidental rebuild/filtering.                          |
| Legacy compaction marker policy not uniformly enforced                |       Broken policy contract | `route_matrix.rs`, `compact_remote.rs`, `context_maintenance.rs`     | `Strip` is declared for refresh/prune but refresh/prune never remove `ResponseItem::Compaction`. `ArtifactKind::CompactionMarker` exists but codec removal does not match it. |
| Timing inferred from injection                                        |  Timing/injection conflation | `compact_timing_from_initial_context_injection`                      | Timing should classify runtime phase. Injection is an effect selected by the policy plan. Current inversion blocks additional timing classes.                                 |
| Stale core injection name                                             |           Semantic narrowing | `InitialContextInjection::BeforeLastUserMessage`                     | Policy says “BeforeLastRealUserOrSummary”; core name says only “user message”. This invites wrong future placement assumptions.                                               |
| Subagent bridge context split                                         | Asset/schema ownership split | policy prompts vs `core/src/continuation_bridge/subagent_context.rs` | Policy prompt treats `<continuation_bridge_subagents>` as authoritative, but core owns the tag and JSON shape.                                                                |
| Tool-call pairing cleanup inside policy                               |    Protocol/runtime law leak | `thread_memory.rs::remove_corresponding_for`                         | It knows specific `ResponseItem` call/output pairings. New tool families can silently miss cleanup unless policy is updated.                                                  |
| Raw retention window in core                                          |         Hidden retention law | `COMPACT_RAW_CONVERSATION_WINDOW_MESSAGES`                           | The “keep latest 5 raw messages when thread memory exists” rule is policy-shaped, but encoded in core.                                                                        |
| Duplicate `content_items_to_text`                                     |      Convenience duplication | policy `artifact_codecs.rs`, core `compact.rs`                       | Low severity, but it is exactly the kind of duplicated seam utility that becomes divergent under upstream churn.                                                              |
| Too-wide policy public surface                                        |             API surface leak | `context-maintenance-policy/src/lib.rs`                              | Low-level tag extraction, insertion helpers, and raw response-item utilities are all public. Core uses them, but the contract is wider than the architecture needs.           |

## 4. Highest-leverage hardening changes

### 1. Replace config/TOML governance dependency with a policy-owned governance axis

**Problem:** `MaintenancePlanningRequest` uses `codex_config::config_toml::GovernancePathVariant`. This makes the policy crate depend on config schema.

**Recommended:** add a policy enum, for example:

```rust
pub enum GovernancePolicyMode {
    Off,
    StrictShadow,
    StrictEnforce,
}
```

or, if the only route-relevant distinction is thread-memory eligibility:

```rust
pub enum ThreadMemoryGovernance {
    Disabled,
    Enabled,
}
```

Then change `core/src/context_maintenance_runtime.rs` to translate core config into this policy enum.

**Why it improves expansion/ingestability:** config/TOML changes no longer touch policy. Governance overlays become explicit policy input instead of deserialization leakage.

**Implementation risk:** low. The change is localized to `contracts.rs`, `route_matrix.rs`, policy tests, and `context_maintenance_runtime.rs`.

---

### 2. Make artifact request semantics executable, not documentary

**Problem:** `ArtifactRequest` includes `ArtifactLifetime`, but core strips it to `ArtifactKind`. Artifact requiredness is also absent from the plan.

**Recommended:** keep full `ArtifactRequest` in `RuntimeMaintenancePlan`, and extend it minimally:

```rust
pub struct ArtifactRequest {
    pub kind: ArtifactKind,
    pub lifetime: ArtifactLifetime,
    pub requiredness: ArtifactRequiredness,
}

pub enum ArtifactRequiredness {
    Required,
    BestEffort,
}
```

Set thread memory as `Required`, continuation bridge as `BestEffort`, and prune manifest as `Required`.

**Why it improves expansion/ingestability:** adding a new artifact family should not require rediscovering failure semantics in three core files. Lifetime and requiredness become policy contract, not comments enforced by convention.

**Implementation risk:** low to medium. The core generation paths need to consume full requests instead of `requests_artifact(kind)` booleans.

---

### 3. Centralize deterministic plan application for drops and legacy markers

**Problem:** `drop_prior_artifact_kinds` and `LegacyCompactionMarkerPolicy` are not applied uniformly. Refresh/prune execute artifact drops but ignore legacy compaction marker stripping. Compact paths do not call the drop list directly.

**Recommended:** add one policy-owned deterministic helper, not a service framework:

```rust
pub struct ArtifactDispositionPlan<'a> {
    pub drop_prior_artifact_kinds: &'a [ArtifactKind],
    pub legacy_compaction_marker_policy: LegacyCompactionMarkerPolicy,
}

pub fn apply_artifact_disposition(
    items: Vec<ResponseItem>,
    plan: ArtifactDispositionPlan<'_>,
) -> (Vec<ResponseItem>, usize)
```

It should:

* prune superseded tagged artifacts;
* remove requested prior artifact kinds;
* remove `ResponseItem::Compaction` when marker policy is `Strip`.

Alternatively, teach `remove_artifact_kind(..., ArtifactKind::CompactionMarker)` to match `ResponseItem::Compaction`, and ensure route plans include `CompactionMarker` in the drop list when `Strip` is intended. The first option is cleaner because marker policy remains explicit.

**Why it improves expansion/ingestability:** there is one executor for carry/drop law. New artifact families and marker policies stop reopening `context_maintenance.rs`, `compact_remote.rs`, and tests independently.

**Implementation risk:** medium. Snapshot behavior may change where refresh/prune currently preserve legacy compaction markers despite policy saying `Strip`.

---

### 4. Decouple compact timing from initial-context injection

**Problem:** `compact_timing_from_initial_context_injection` derives `MaintenanceTiming` from `InitialContextInjection`. This makes a policy output shape drive route selection.

**Recommended:** replace the caller-facing compact input with explicit timing, for example:

```rust
pub enum CompactInvocationTiming {
    TurnBoundary,
    IntraTurn,
}
```

Core call sites should pass timing based on runtime phase. The policy plan should then return `ContextInjectionPolicy`, and core should obey it. After that, either remove `InitialContextInjection` or make it a private placement adapter with names matching policy semantics.

**Why it improves expansion/ingestability:** adding `PreSampling`, `PostTool`, `Resume`, or other timing classes will not require inventing fake injection variants. Timing remains runtime phase; injection remains policy decision.

**Implementation risk:** medium. It touches compact call sites and snapshot tests, but the conceptual change is small.

---

### 5. Move continuation-bridge supplemental block ownership into policy

**Problem:** policy prompts mention `<continuation_bridge_subagents>`, but core owns the tag and JSON structure in `core/src/continuation_bridge/subagent_context.rs`.

**Recommended:** move the tag constant, serializable snapshot struct, and response-item builder into the policy crate. Core should only translate `AgentStatus` and runtime subagent records into policy-owned snapshot values.

Example boundary:

```rust
pub struct ContinuationBridgeSubagentSnapshot {
    pub agent_id: String,
    pub thread_id: String,
    pub nickname: String,
    pub role: String,
    pub status: ContinuationBridgeSubagentStatus,
}

pub fn continuation_bridge_subagents_item(
    snapshots: Vec<ContinuationBridgeSubagentSnapshot>,
) -> CodexResult<Option<ResponseItem>>
```

**Why it improves expansion/ingestability:** the prompt, schema expectations, tag name, and JSON shape are in one crate. Core keeps runtime lookup only.

**Implementation risk:** low to medium. The shape is small and currently isolated.

---

### 6. Move raw retention policy into the route plan

**Problem:** the “retain latest 5 raw conversation messages when thread memory is present” rule is encoded in core via `COMPACT_RAW_CONVERSATION_WINDOW_MESSAGES`.

**Recommended:** add a typed retention directive to `MaintenancePolicyPlan`, or attach it to the `ThreadMemory` artifact request:

```rust
pub enum RawHistoryRetention {
    Unchanged,
    LatestRawConversationMessages(usize),
}
```

Core still supplies the `is_raw_conversation_message` predicate, but the policy crate owns when and how much retention is requested.

**Why it improves expansion/ingestability:** future thread-memory variants or governance overlays can change carry-forward behavior without editing compact/refresh/prune orchestration.

**Implementation risk:** medium because snapshots may encode exact retained history.

---

### 7. Isolate protocol pairing law used by thread-memory trimming

**Problem:** `thread_memory.rs::remove_corresponding_for` knows that `FunctionCall` pairs with `FunctionCallOutput`, `ToolSearchCall` pairs with `ToolSearchOutput`, `CustomToolCall` pairs with `CustomToolCallOutput`, and `LocalShellCall` pairs with `FunctionCallOutput`.

**Recommended:** do not build a generic graph framework. Use one of two small options:

* Move this relation into `codex-protocol` as a named utility if protocol ownership is acceptable.
* Or inject a core-owned pairing predicate/helper into policy trimming.

The second option keeps policy less coupled to protocol variant churn, but widens the policy API. The first option touches an upstream-shaped crate but names the law where `ResponseItem` is defined.

**Why it improves expansion/ingestability:** new tool-call variants will have one obvious place to update.

**Implementation risk:** medium. This is behavior-sensitive and should be covered by focused tests.

---

### 8. Narrow the public policy API after the executor boundary is clear

**Problem:** `lib.rs` publicly exports low-level helpers like `extract_tagged_payload`, `has_tagged_block`, `tagged_artifact_kind_from_text`, insertion helpers, and raw retention helpers.

**Recommended:** after adding a small plan-application helper, make low-level tag and history utilities `pub(crate)` unless core truly needs them. Public API should mostly be:

* planning contract;
* artifact generation/parsing functions;
* one or two deterministic plan/history application helpers;
* test fixtures only under test support.

**Why it improves expansion/ingestability:** fewer public seams harden the crate as a policy boundary rather than a bag of utilities.

**Implementation risk:** low if sequenced after the call sites are consolidated.

## 5. Changes to avoid

* Do not move `Session`, `TurnContext`, model-client streaming, telemetry, tool routing, or event emission into the policy crate. Those are runtime concerns.
* Do not make app-server, TUI, or protocol crates depend on `codex-context-maintenance-policy`. Keep policy behind core.
* Do not introduce a generic context-maintenance “service framework” that owns everything. The current problem is split law, not lack of an object hierarchy.
* Do not replace `ResponseItem` with a mirror AST just to avoid `codex-protocol`. That would create a second protocol law surface. Keep direct `ResponseItem` use where the artifact is literally a response item; isolate only the runtime-specific variant semantics.
* Do not merge state DB `memory_mode` with context-maintenance `ThreadMemory` artifact semantics. One is durable thread eligibility/pollution state; the other is a model-visible continuity artifact.
* Do not generalize artifact families into a plugin registry yet. A typed enum plus explicit plan fields is sufficient.
* Do not move compact summary generation wholesale into policy. The vanilla/upstream compaction summary machinery still belongs in core unless its carry/drop behavior is being formalized as policy.
* Do not delete transitional snapshot tests until target-law tests exist. Several current tests explicitly lock known transitional behavior around pre-turn compaction excluding incoming user input.

## 6. Minimal next PR sequence

### PR 1 — Remove `codex-config` from the policy crate

* Add policy-owned governance input type.
* Change `MaintenancePlanningRequest`.
* Update `route_matrix.rs` and policy tests.
* Change `core/src/context_maintenance_runtime.rs` to map core config governance into the policy type.
* Remove `codex-config` from `context-maintenance-policy/Cargo.toml`.

This is the cleanest boundary win with the lowest risk.

### PR 2 — Preserve artifact request metadata through core

* Change `RuntimeMaintenancePlan` to retain `ArtifactRequest`, not only `ArtifactKind`.
* Add `ArtifactRequiredness`.
* Replace ad hoc “thread memory fatal, bridge optional” checks with plan-driven requiredness.
* Keep generation logic in core.

This makes artifact family meaning policy-owned without moving runtime execution.

### PR 3 — Fix legacy compaction marker enforcement

* Add focused tests showing current refresh/prune behavior against `ResponseItem::Compaction`.
* Add a policy helper or teach artifact disposition to strip `ResponseItem::Compaction` when requested.
* Apply it in `/refresh` and `/prune`.
* Avoid changing remote compact behavior beyond replacing the bool with the policy enum or helper.

This closes the strongest current broken contract.

### PR 4 — Replace timing-from-injection with explicit timing

* Introduce a core runtime timing input.
* Route compact calls by explicit timing.
* Treat `ContextInjectionPolicy` only as policy output.
* Rename or remove `InitialContextInjection::BeforeLastUserMessage`.

This removes the second timing law surface.

### PR 5 — Move continuation bridge subagent supplemental block into policy

* Add policy-owned snapshot/status/tag/response-item builder.
* Keep `AgentStatus` translation and session lookup in core.
* Update continuation-bridge tests to assert the tag and JSON shape in policy.

This fixes the prompt/schema ownership split.

### PR 6 — Add policy-owned retention directive

* Move the raw-retention window out of `core/src/compact.rs`.
* Add typed retention directive to `MaintenancePolicyPlan`.
* Core continues supplying `is_raw_conversation_message`.

This hardens future carry-forward changes.

### PR 7 — Public API narrowing and test taxonomy cleanup

* Hide low-level helpers no longer needed by core.
* Keep policy crate tests as target-law tests.
* Keep `core/src/compaction_policy_matrix_tests.rs` as adapter/translation tests, not duplicate doctrine.
* Move pure insertion/retention tests out of core where policy already owns the algorithm.
* Rename or group snapshot tests that lock transitional current behavior, especially pre-turn compaction cases that exclude incoming user input.

Net assessment: the crate extraction is a good architectural move, but the next step is not a larger abstraction. The next step is making the existing plan fully executable: governance input must be policy-owned, artifact request metadata must survive into core, marker/drop/retention rules must have one executor, and timing must stop being inferred from context injection.
