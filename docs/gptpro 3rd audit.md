## 1. Current-state assessment: context-maintenance as worked example

### Judgment

**Observed:** the context-maintenance extraction is now the strongest modularization pattern in the fork. The crate boundary is substantially cleaner than the older compaction-policy shape.

`context-maintenance-policy/Cargo.toml` now depends on:

* `codex-protocol`
* `serde`
* `serde_json`
* `thiserror`
* `tracing`

It does **not** depend on `codex-core`, `codex-config`, TUI, app-server, runtime/session state, or CLI code. That is the correct dependency direction.

### What is self-contained now

The policy crate owns the important semantic law:

* route/action/timing/engine concepts in `context-maintenance-policy/src/contracts.rs`
* route planning in `context-maintenance-policy/src/route_matrix.rs`
* artifact kinds, lifetimes, requiredness, and retention directives in `contracts.rs`
* history disposition in `context-maintenance-policy/src/artifact_codecs.rs`
* remote compact history shaping in `context-maintenance-policy/src/history_shape.rs`
* continuation bridge prompts/schema/source selection/response item construction in `context-maintenance-policy/src/continuation_bridge/`
* thread memory prompt/schema/source selection/trimming/response item construction in `context-maintenance-policy/src/thread_memory.rs`

The runtime adapter is mostly clean:

* `core/src/context_maintenance_runtime.rs` translates `CompactionEngine`, `GovernancePathVariant`, and compact invocation timing into policy-owned inputs.
* `RuntimeMaintenancePlan` preserves full `ArtifactRequest`, including `ArtifactRequiredness`.
* `execute_requested_artifact` applies required vs best-effort behavior centrally.
* `CompactInvocationTiming` is explicit now; timing is no longer derived from context-injection placement.

### Remaining context-maintenance weaknesses

These are follow-up cleanup, not evidence that the extraction failed:

1. **Policy API still broad.**
   `context-maintenance-policy/src/lib.rs` exports many low-level helpers: retention helpers, insertion helpers, payload parse/build helpers, source-selection helpers, disposition helpers. Some are legitimate public contract; some should likely become `pub(crate)` once production call sites are stable.

2. **Governance-off semantics remain ambiguous.**
   In `route_matrix.rs`, `ThreadMemoryGovernance::Disabled` removes requested `ThreadMemory` artifacts but does not necessarily mean “drop/ignore existing durable thread memory.” That may be intentional, but the enum name is stronger than the implementation.

3. **Core tests still duplicate doctrine.**
   `core/src/compaction_policy_matrix_tests.rs` still mirrors route-law assertions that now belong primarily in `context-maintenance-policy/src/tests.rs`. Core tests should mostly verify adapter translation and runtime wiring.

### Reusable lessons

The reusable method is not “make a crate for every fork feature.” The actual lesson is narrower:

* identify a durable semantic law surface;
* move pure law first;
* keep runtime/session/model I/O in adapters;
* move prompts/schemas/tags/tool contracts with the semantic owner;
* activate live behavior only after ownership moved;
* then harden remaining leaks: config-schema imports, duplicate constants, low-level public helpers, and tests that create second doctrine.

That method applies well to E-witness/sub-agent observability. It does **not** apply cleanly to `codex cloud` in the current fork because that stack is not fork-local relative to vanilla `0.121`.

---

## 2. Generalized fork modularization method

### 2.1 Detection criteria: when a feature deserves a module/crate

A feature deserves a dedicated module or crate when most of these are true:

| Criterion                                         | Meaning                                                                                                                         |
| ------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| Durable semantic law exists                       | There are rules that should survive runtime refactors: route law, event reduction law, tool-surface law, artifact lifetime law. |
| Law appears in multiple hot files                 | The feature touches `core/src/codex.rs`, tool registration, config plumbing, runtime handlers, and tests.                       |
| It has model-visible contracts                    | Prompts, JSON schemas, tool names, artifact tags, output schemas, or serialized enums.                                          |
| It can be expressed without `Session`             | The core logic can take typed inputs and return typed outputs without owning runtime I/O.                                       |
| It needs target-law tests                         | The behavior should be tested independently from the current runtime flow.                                                      |
| Future expansion would otherwise reopen hot files | New artifact families, new event phases, new tool variants, new route overlays, etc.                                            |

A feature should **not** get a crate when:

* it is only a CLI dispatch branch;
* it is already upstream-owned and identical to vanilla;
* its logic is mostly I/O/UI/client orchestration;
* extraction would require moving `Session`, `TurnContext`, model streaming, TUI state, or auth managers into the crate;
* the new crate would be a generic “fork hub” with no single semantic axis.

### 2.2 Sequence

#### Step 1 — Compare against vanilla first

Before modularizing, classify files as:

* **fork-only**
* **fork-modified hot upstream files**
* **unchanged upstream files**
* **already upstream-owned modules**

For this task, that distinction is decisive: E-witness is fork-local; `cloud-tasks` is not.

#### Step 2 — Name the semantic law

Do not start with crate boundaries. Start with the law.

For context-maintenance, the law was:

* route matrix;
* artifact family meaning;
* carry/drop/retention behavior;
* bridge/thread-memory source shaping;
* legacy compaction marker behavior.

For E-witness, the law is:

* event-to-progress reduction;
* material-progress sequence semantics;
* progress phase/blocker/output schema;
* progress wait match semantics;
* thread-spawn tool-surface containment.

For `codex cloud`, current code mostly has no fork-local law.

#### Step 3 — Lock current behavior

Use tests to lock the current behavior before moving ownership.

Test classes should be separated:

* **current-behavior locks:** preserve current runtime behavior during extraction;
* **target-law tests:** define the desired module-owned doctrine;
* **adapter tests:** verify translation from core/config/session into module-owned inputs;
* **live runtime tests:** verify production wiring still works.

Avoid using core tests to restate all module doctrine.

#### Step 4 — Extract the pure first slice

The first slice should not include runtime I/O. It should include:

* typed enums;
* typed input/output structs;
* reducer or planner;
* deterministic classifiers/selectors;
* artifact/tool/schema constants;
* small serialization helpers.

It should not include:

* `Session`;
* `TurnContext`;
* model streaming;
* auth manager;
* TUI/app state;
* CLI command execution;
* thread lifecycle orchestration.

#### Step 5 — Add a thin adapter in vanilla-touching code

The adapter translates runtime state into module-owned inputs and executes effects.

Good adapter examples:

* `core/src/context_maintenance_runtime.rs`
* a future `core/src/agent/progress_runtime.rs` or retained `AgentControl` methods that translate live threads/events into observability calls
* `cli/src/main.rs` dispatching to `codex_cloud_tasks::run_main`

Bad adapters:

* adapters that contain policy decisions;
* adapters that duplicate string constants or schema variants;
* adapters that infer module behavior from runtime side effects.

#### Step 6 — Move model-visible contracts with ownership

Prompts, JSON schemas, output schemas, artifact tags, tool names, enum string lists, and marker strings should sit with the module that owns their meaning.

Context-maintenance now mostly follows this.

E-witness currently does not: `AgentProgressPhase` is in `core/src/agent/progress.rs`, while the phase schema strings are duplicated in `tools/src/agent_progress_tool.rs`.

#### Step 7 — Activate live law only after ownership moved

Once the module owns the law, core should call the module. Do not leave the old law path active in parallel.

For context-maintenance, this meant route planning and history disposition moved before core consumed the plan.

For E-witness, this means tool handlers and `Session::deliver_event_raw` should call a module-owned reducer/registry, not keep a core-local reducer plus tool-schema duplicates.

#### Step 8 — Harden remaining seams

After extraction, harden the boundary:

* remove config/TOML imports from policy crates;
* preserve typed request metadata through runtime adapters;
* move protocol-generic helpers to `codex-protocol`;
* collapse duplicated constants;
* narrow public API;
* move doctrine tests into the module crate;
* keep core tests as adapter/runtime tests.

### 2.3 “Done enough” criteria

A feature module is done enough when:

* it has no dependency on `codex-core`;
* it has no dependency on TOML/config schema unless config itself is the domain;
* hot upstream files contain translation/execution only;
* model-visible strings and schemas have one owner;
* adding a new semantic variant mostly touches the module and one adapter;
* core tests do not duplicate module doctrine;
* public API exposes typed concepts, not arbitrary low-level helpers.

### 2.4 Anti-patterns

Avoid these:

* generic hub crates with unrelated law inside;
* mirror ASTs for protocol types when the real artifact is a `ResponseItem`;
* moving runtime services into policy crates;
* keeping duplicate schema strings in tools and core;
* extracting everything touched by a feature instead of only its semantic law;
* creating a crate for an upstream-owned stack with no fork delta;
* using tests in hot core files to preserve old transitional helper APIs.

---

## 3. Target A: E-witness / sub-agent observability

## 3.1 Verdict

**Recommended:** create a dedicated observability module/crate, but do **not** create a broad “multi-agent feature crate.”

The real boundary is not all sub-agent control. The real boundary is:

> normalized sub-agent progress observation and wait semantics, plus the model-visible progress tool contract.

Thread-spawn containment is adjacent but should stay primarily in `codex-tools`, because it is tool-surface policy rather than progress-reducer law.

A reasonable crate name:

* `codex-agent-observability`
* or `codex-subagent-observability`

I would prefer `codex-agent-observability`; the reducer can observe any agent thread, even though current usage is sub-agent/E-witness.

---

## 3.2 Observed current touch points

### Fork-only or heavily fork-owned files

`core/src/agent/progress.rs`

* Fork-only.
* 1038 lines.
* Owns `AgentProgressPhase`, `AgentBlockReason`, `AgentActiveWorkKind`, `AgentProgressSnapshot`, `ProgressRegistry`, event reduction, seq advancement, stall detection, recent update limits, and tests.
* Imports `codex_protocol::protocol::EventMsg`, `AgentReasoningEvent`, and `ThreadId`.
* The only core-specific import is effectively replaceable: `crate::agent::status::is_final`.

`core/src/tools/handlers/agent_progress.rs`

* Fork-only.
* Owns runtime handler execution for `inspect_agent_progress` and `wait_for_agent_progress`.
* Depends on `Session`, `TurnContext`, `resolve_agent_target`, handler output machinery, and tool invocation context.
* Should remain core-side adapter/runtime code.

`tools/src/agent_progress_tool.rs`

* Fork-only.
* Owns tool specs and output schemas for `inspect_agent_progress` and `wait_for_agent_progress`.
* Duplicates progress phase strings manually in `phase_enum_schema`.

### Fork modifications in hot upstream-shaped files

`core/src/codex.rs`

* `Session::deliver_event_raw` records every emitted event through `agent_control.record_progress_event(self.conversation_id, &event.msg)`.
* This is a hot event path. The hook is acceptable; the reducer law should not live here.

`core/src/agent/control.rs`

* Seeds progress when spawning/resuming agents.
* Provides `subscribe_progress_seq`, `record_progress_event`, `seed_agent_progress`, `inspect_agent_progress`, and `list_subagents`.
* This is adapter/lifecycle code. It should call an observability crate, not own reducer semantics.

`core/src/thread_manager.rs`

* Stores `progress_registry: ProgressRegistry` in `ThreadManagerState`.
* Initializes/removes registry state.
* This storage location is acceptable, but the type should come from the observability crate.

`core/src/tools/spec.rs`

* Maps `ToolHandlerKind::InspectAgentProgress` and `ToolHandlerKind::WaitForAgentProgress` to core handlers.
* This is acceptable adapter wiring.

`core/src/tools/router.rs`

* Hardcodes `"inspect_agent_progress" | "wait_for_agent_progress"` in `model_visible_in_code_mode_only`.
* This is a string leak. Use owned constants.

`tools/src/tool_config.rs`

* Adds `spawn_agent_enabled` and `request_user_input_enabled`.
* Encodes thread-spawn containment:

  * thread-spawned subagents do not get `spawn_agent`;
  * all subagents do not get `request_user_input`.

`tools/src/tool_registry_plan.rs`

* Registers inspect/wait progress tools under `config.collab_tools`.
* Suppresses `spawn_agent` registration when `config.spawn_agent_enabled` is false.
* Suppresses `request_user_input` when `config.request_user_input_enabled` is false.

`tools/src/tool_registry_plan_types.rs`

* Adds `ToolHandlerKind::InspectAgentProgress` and `ToolHandlerKind::WaitForAgentProgress`.

---

## 3.3 Boundary classification

### Should become module-owned

Move or centralize these:

1. `AgentProgressPhase`
2. `AgentBlockReason`
3. `AgentActiveWorkKind`
4. `AgentActiveWork`
5. `AgentProgressSnapshot`
6. `ProgressRegistry`
7. event-to-progress reducer
8. material-progress sequence rules
9. stall calculation
10. wait-match classification
11. progress tool names:

* `inspect_agent_progress`
* `wait_for_agent_progress`

12. progress phase string list used by the tool schema
13. output-schema semantic constants, or at least the enum/string constants that feed them

The crate can depend on:

* `codex-protocol`
* `serde`
* `tokio`
* possibly `tracing`

It should not depend on:

* `codex-core`
* `codex-tools`
* `codex-config`
* TUI/app-server/CLI crates

### Should remain adapter-side

Keep these in core:

* resolving an agent target from a tool argument: `core/src/agent/agent_resolver.rs`
* inspecting live thread lifecycle through `AgentControl`
* seeding/removing progress during thread spawn/resume/remove
* hooking `Session::deliver_event_raw`
* tool handler runtime execution in `core/src/tools/handlers/agent_progress.rs`
* tool registry handler mapping in `core/src/tools/spec.rs`

Keep these in `codex-tools`:

* actual `ToolSpec` construction;
* `ToolHandlerKind`;
* tool registry planning;
* thread-spawn tool-surface policy.

`codex-tools` can depend on `codex-agent-observability` for constants/enums without creating a core dependency cycle.

---

## 3.4 Correctness issues

### A1 — Tool schema can drift from runtime enum

**Observed:** `AgentProgressPhase` is defined in `core/src/agent/progress.rs`, but `tools/src/agent_progress_tool.rs` manually repeats the phase string list in `phase_enum_schema`.

**Why it matters:** a future phase can be added to the reducer but omitted from the schema, or added to the schema but not deserializable by the handler.

**Recommended:** move `AgentProgressPhase` and a canonical phase-name list to `codex-agent-observability`. Make `tools/src/agent_progress_tool.rs` derive the schema enum list from that owner.

**Risk:** low.

---

### A2 — Progress tool names are duplicated as strings

**Observed:** tool names appear in:

* `tools/src/agent_progress_tool.rs`
* `tools/src/tool_registry_plan.rs`
* `core/src/tools/router.rs`
* handler/tests

The router specifically hardcodes:

```rust
"inspect_agent_progress" | "wait_for_agent_progress"
```

**Recommended:** define constants in the observability crate:

```rust
pub const INSPECT_AGENT_PROGRESS_TOOL_NAME: &str = "inspect_agent_progress";
pub const WAIT_FOR_AGENT_PROGRESS_TOOL_NAME: &str = "wait_for_agent_progress";
```

Use them from `codex-tools` and `codex-core`.

**Risk:** low.

---

### A3 — Reducer law is hidden inside core

**Observed:** `core/src/agent/progress.rs` is fork-only but lives under core. It owns the law for interpreting `EventMsg` into phase, blocker, active work, recent updates, terminal states, and sequence advancement.

**Why it matters:** upstream churn in core agent lifecycle or event emission now forces maintainers to reason about reducer doctrine inside core. The file is not upstream-shaped, but its placement makes core the owner of observability semantics.

**Recommended:** move the reducer and registry to `codex-agent-observability`.

**Risk:** medium-low. The code is already mostly protocol-driven and self-contained.

---

## 3.5 Boundary/ownership leaks

### A4 — Tool-surface containment is currently implicit booleans

**Observed:** `tools/src/tool_config.rs` derives:

* `spawn_agent_enabled`
* `request_user_input_enabled`

from `SessionSource`.

Then `tools/src/tool_registry_plan.rs` uses those booleans to decide whether to include/register tools.

**Why it matters:** the law is real: thread-spawned subagents may observe/control siblings but may not recursively spawn agents; subagents also do not get `request_user_input`. That law is not named as a policy object.

**Recommended:** keep it in `codex-tools`, but make it explicit:

```rust
pub struct CollabToolSurfacePolicy {
    pub include_progress_observability: bool,
    pub include_spawn_agent: bool,
    pub include_request_user_input: bool,
    pub include_agent_control: bool,
}
```

Construct it from `SessionSource` and feature flags in one helper. `ToolsConfig::new` should consume the helper instead of embedding the matches inline.

**Risk:** low.

---

### A5 — Core spec tests duplicate tool-planning doctrine

**Observed:** `core/src/tools/spec_tests.rs` dynamically patches expected tool lists with progress tools and asserts thread-spawn suppression behavior. `tools/src/tool_registry_plan_tests.rs` also owns tool-planning behavior.

**Recommended:** move most doctrine to `tools/src/tool_registry_plan_tests.rs` and `tools/src/tool_config_tests.rs`. Keep core tests for handler registration and runtime wiring only.

**Risk:** low.

---

### A6 — `ProgressRegistry` storage is acceptable, but ownership is not

**Observed:** `ThreadManagerState` stores `progress_registry: ProgressRegistry`.

That storage location is fine. The type should come from the observability crate. `ThreadManagerState` should not care how the reducer works.

**Recommended:** after moving the crate, `core/src/thread_manager.rs` should import:

```rust
use codex_agent_observability::ProgressRegistry;
```

**Risk:** low.

---

## 3.6 Proposed Target A crate shape

### New crate

`agent-observability/`

Cargo package:

```toml
name = "codex-agent-observability"
```

Dependencies:

```toml
codex-protocol = { workspace = true }
serde = { workspace = true, features = ["derive"] }
tokio = { workspace = true, features = ["sync"] }
tracing = { workspace = true } # optional
```

### Owned modules

Suggested files:

```text
agent-observability/src/lib.rs
agent-observability/src/progress.rs
agent-observability/src/tool_contract.rs
agent-observability/src/wait.rs
```

### Public API

Keep it narrow:

```rust
pub enum AgentProgressPhase { ... }
pub enum AgentBlockReason { ... }
pub enum AgentActiveWorkKind { ... }

pub struct AgentActiveWork { ... }
pub struct AgentProgressSnapshot { ... }

pub struct ProgressRegistry { ... }

impl ProgressRegistry {
    pub fn seed(&self, thread_id: ThreadId);
    pub fn record_event(&self, thread_id: ThreadId, event: &EventMsg);
    pub fn subscribe_seq(&self, thread_id: ThreadId) -> watch::Receiver<u64>;
    pub fn inspect(
        &self,
        thread_id: ThreadId,
        lifecycle_status: AgentStatus,
        stalled_after: Duration,
    ) -> AgentProgressSnapshot;
    pub fn remove(&self, thread_id: &ThreadId);
}

pub const INSPECT_AGENT_PROGRESS_TOOL_NAME: &str;
pub const WAIT_FOR_AGENT_PROGRESS_TOOL_NAME: &str;

pub const AGENT_PROGRESS_PHASE_NAMES: &[&str];
```

Wait classification can also be module-owned:

```rust
pub enum WaitForAgentProgressMatchReason {
    SeqAdvanced,
    AlreadySatisfied,
    PhaseReached,
}

pub fn classify_wait_match(
    snapshot: &AgentProgressSnapshot,
    baseline_seq: u64,
    until_phases: &[AgentProgressPhase],
) -> Option<WaitForAgentProgressMatchReason>;
```

The handler can still own timeout parsing and `tokio::time` waiting because that is runtime execution.

---

## 3.7 Target A PR sequence

### PR A1 — Behavior locks before extraction

High value.

Add or consolidate tests for:

* phase schema list matches `AgentProgressPhase`;
* progress tool names are single-sourced;
* thread-spawned subagent lacks `spawn_agent`;
* subagents lack `request_user_input`;
* thread-spawned subagent still has inspect/wait progress tools;
* material-progress seq advances only on meaningful events;
* terminal events update phase and status.

Most of these should live in:

* `core/src/agent/progress.rs` temporarily;
* `tools/src/tool_registry_plan_tests.rs`;
* `tools/src/tool_config_tests.rs`.

Core runtime tests should stay minimal.

### PR A2 — Create `codex-agent-observability` and move reducer/types

High value.

Move from `core/src/agent/progress.rs`:

* DTOs;
* phases/block reasons;
* reducer;
* registry;
* tests.

Update:

* workspace `Cargo.toml`;
* `core/Cargo.toml`;
* imports in `core/src/agent/control.rs`;
* imports in `core/src/thread_manager.rs`;
* imports in `core/src/tools/handlers/agent_progress.rs`.

Keep behavior unchanged.

### PR A3 — Single-source progress tool contract

High value.

Move tool names and enum-string constants into `codex-agent-observability`.

Update:

* `tools/src/agent_progress_tool.rs`;
* `tools/src/tool_registry_plan.rs`;
* `core/src/tools/router.rs`;
* tests.

This removes the most concrete schema drift risk.

### PR A4 — Name thread-spawn tool-surface policy in `codex-tools`

High value but smaller.

Add a policy helper inside `codex-tools`, not a new crate:

```rust
CollabToolSurfacePolicy::from_session_source_and_features(...)
```

Update:

* `tools/src/tool_config.rs`;
* `tools/src/tool_registry_plan.rs`;
* `tools/src/tool_config_tests.rs`;
* `tools/src/tool_registry_plan_tests.rs`.

Keep `core/src/tools/spec.rs` as handler wiring only.

### PR A5 — Test taxonomy cleanup

Medium value.

Move doctrine tests out of:

* `core/src/tools/spec_tests.rs`

where they duplicate `codex-tools` planning tests.

Retain core tests only for:

* handler kind maps to handler implementation;
* `Session::deliver_event_raw` records progress;
* inspect/wait handler works through real runtime target resolution.

### PR A6 — Optional API narrowing

Lower value.

After extraction, narrow the observability crate public API. Do not export internal reducer helpers unless `core` or `tools` uses them.

---

## 4. Target B: headless cloud / `codex cloud` task surface

## 4.1 Verdict

**Observed:** Target B is mis-scoped as a fork-local modularization target for the current source state.

Comparing the updated fork against vanilla `0.121`:

* `cli/src/main.rs` is identical.
* `cloud-tasks/` is identical.
* `cloud-tasks-client/` is identical.
* `cloud-tasks-mock-client/` is identical.
* `cloud-requirements/` is also identical.

So this is not currently a custom fork feature family. It is an upstream-owned stack already present in the vanilla baseline.

**Recommended:** do not create a fork-local cloud module or hub around it.

---

## 4.2 Current cloud boundary

The current upstream-owned boundary is already clear:

### CLI integration

`cli/src/main.rs`

* `Subcommand::Cloud(CloudTasksCli)` at the root CLI.
* Alias: `cloud-tasks`.
* Dispatch branch rejects remote mode, prepends root config flags into `cloud_cli.config_overrides`, then calls:

```rust
codex_cloud_tasks::run_main(cloud_cli, arg0_paths.codex_linux_sandbox_exe.clone()).await?;
```

This is a thin adapter and should remain thin.

### Cloud task app crate

`cloud-tasks/`

Owns:

* CLI subcommands in `cloud-tasks/src/cli.rs`;
* TUI/app flow in `cloud-tasks/src/app.rs`, `ui.rs`, `lib.rs`;
* backend initialization in `cloud-tasks/src/lib.rs`;
* utility/auth helpers in `cloud-tasks/src/util.rs`.

### Backend seam

`cloud-tasks-client/src/api.rs`

Owns:

* `CloudBackend` trait;
* task DTOs;
* task status;
* diff/apply/create/list interfaces.

`cloud-tasks-mock-client/`

Owns mock backend implementation.

This is already the correct seam for client/mock/backend separation.

---

## 4.3 Correctness issue worth noting, but not fork-local modularization

### B1 — CLI config overrides are accepted but effectively ignored by cloud auth loading

**Observed:**

`cli/src/main.rs` prepends root config overrides into `cloud_cli.config_overrides`.

`cloud-tasks/src/cli.rs` stores:

```rust
pub config_overrides: CliConfigOverrides
```

But `cloud-tasks/src/lib.rs::run_main` destructures `Cli { .. } = cli`, and `cloud-tasks/src/util.rs::load_auth_manager` calls:

```rust
Config::load_with_cli_overrides(Vec::new()).await.ok()?
```

with a TODO saying overrides should be passed.

**Why it matters:** root config flags are threaded into the cloud CLI object but not used when loading config/auth.

**Recommended:** fix this only if the fork cares about cloud behavior. The fix belongs in the existing upstream cloud stack, not in a new fork module.

Minimal shape:

* pass `cli.config_overrides` into `init_backend`;
* pass config overrides into `util::load_auth_manager`;
* avoid `Vec::new()`.

**Risk:** low to medium, because auth/config behavior is user-visible.

---

## 4.4 Boundary/ownership observations

### B2 — Mock client dependency is production dependency

**Observed:** `cloud-tasks/Cargo.toml` has:

```toml
# TODO: codex-cloud-tasks-mock-client should be in dev-dependencies.
codex-cloud-tasks-mock-client = { workspace = true }
```

**Recommended:** optional cleanup. Move behind dev dependency or feature-gated debug path when practical.

**Risk:** low, but it is upstream-owned cleanup, not fork modularization.

---

### B3 — Do not group with `cloud-requirements`

**Observed:** `cloud-requirements/` is also identical to vanilla and is not required to explain the current `codex cloud` task boundary.

**Recommended:** keep Target B scoped to:

* `cli/src/main.rs`
* `cloud-tasks/`
* `cloud-tasks-client/`
* `cloud-tasks-mock-client/`

Do not merge cloud requirements into this feature family unless future fork changes create a shared semantic law.

---

## 4.5 Target B PR sequence

No modularization PR is recommended.

Optional cleanup sequence:

### PR B1 — Inventory correction

High value for fork process, low code impact.

Update fork inventory/docs to state:

* `codex cloud` task stack is upstream-owned in vanilla `0.121`;
* current fork has no delta in `cloud-tasks`, `cloud-tasks-client`, or `cloud-tasks-mock-client`;
* revalidation during ingest should treat it as upstream unless future fork changes appear.

### PR B2 — Config override plumbing

Medium value if cloud is actively used.

Thread `Cli.config_overrides` through:

* `cloud-tasks/src/lib.rs::run_main`
* backend initialization
* `cloud-tasks/src/util.rs::load_auth_manager`

### PR B3 — Mock dependency cleanup

Low value.

Move `codex-cloud-tasks-mock-client` out of production dependencies or feature-gate it.

---

## 5. Hub / meta-module recommendation

## 5.1 Judgment

**Recommended:** do not introduce a generic fork-side hub/meta-module now.

The fork is better served by:

* per-feature modules with one semantic axis;
* thin runtime adapters in hot upstream files;
* shared protocol-level helpers only when the helper is genuinely protocol-owned.

A generic hub would likely become a second hidden law surface.

---

## 5.2 When a hub would help

A hub can help only when it is not a policy owner.

Acceptable hub shape:

```text
fork-runtime-adapters/
```

with no decisions, only dispatch plumbing. For example:

* register fork event observers;
* call per-feature event hooks;
* expose constants for runtime adapter registration.

But it must not decide:

* context-maintenance route behavior;
* agent progress phase transitions;
* thread-spawn tool availability;
* cloud backend behavior;
* governance overlays.

Once it decides those, it becomes an untyped policy aggregator.

---

## 5.3 Why a generic hub is wrong here

The current feature families have unrelated semantic axes:

| Feature                       | Real owner                           |
| ----------------------------- | ------------------------------------ |
| Context maintenance           | `codex-context-maintenance-policy`   |
| E-witness progress            | proposed `codex-agent-observability` |
| Thread-spawn tool containment | `codex-tools`                        |
| Cloud task client/backend     | existing `cloud-tasks*` crates       |
| Protocol helpers              | `codex-protocol`                     |

A generic hub would mix:

* artifact lifecycle law;
* history retention law;
* event reduction law;
* tool-surface law;
* cloud auth/backend law.

That is precisely the hidden-law pattern the context-maintenance work was designed to remove.

---

## 5.4 Domain-specific hubs are acceptable

The correct pattern is domain-specific:

* `codex-context-maintenance-policy` for context-maintenance law;
* proposed `codex-agent-observability` for progress/reducer law;
* `codex-tools` for tool-surface planning;
* `codex-protocol` for protocol-generic utilities.

No feature-agnostic broker is needed.

---

## 6. Final prioritized follow-up sequence

### Priority 1 — Extract E-witness progress reducer into `codex-agent-observability`

This is the highest-value structural work.

Move the semantic core of `core/src/agent/progress.rs` into a crate with no `codex-core` dependency. Keep core as lifecycle/runtime adapter.

Expected benefit:

* reduces fork law embedded in core;
* gives progress phases/seq/stall behavior a testable owner;
* lowers upstream-ingest risk around agent lifecycle churn.

### Priority 2 — Single-source E-witness tool names and phase schema

Move progress tool names and phase-name lists into the observability crate.

Update:

* `tools/src/agent_progress_tool.rs`
* `tools/src/tool_registry_plan.rs`
* `core/src/tools/router.rs`
* handler/tests

Expected benefit:

* removes concrete schema drift risk;
* prevents stringly seams across core/tools.

### Priority 3 — Make thread-spawn tool containment explicit in `codex-tools`

Do not create a separate crate for this.

Add a named `CollabToolSurfacePolicy` or equivalent helper inside `codex-tools`.

Expected benefit:

* makes recursive-spawn containment readable and testable;
* keeps tool-surface law in the crate that already owns tool planning.

### Priority 4 — Clean Target A test taxonomy

Move route/tool-planning doctrine into `codex-tools` and observability crate tests.

Keep core tests for:

* runtime hook;
* handler mapping;
* target resolution;
* inspect/wait end-to-end behavior.

Expected benefit:

* avoids second doctrine in core;
* makes future upstream ingest less confusing.

### Priority 5 — Mark Target B as upstream-owned / no fork modularization

Update fork inventory so `codex cloud` is not treated as a fork-local module unless future diffs appear.

Expected benefit:

* prevents wasted extraction work;
* keeps ingest focus on real fork deltas.

### Priority 6 — Optional cloud cleanup

Only if cloud behavior matters to this fork:

* thread `Cli.config_overrides` into cloud auth/config loading;
* move mock client dependency behind dev/feature gating.

Expected benefit:

* real correctness cleanup, but not a modularization prerequisite.

### Priority 7 — No generic fork hub

Do not build it.

The current architecture should be:

```text
hot upstream files
    -> thin adapters
        -> domain-specific fork modules/crates
```

not:

```text
hot upstream files
    -> generic fork hub
        -> mixed feature law
```

The hub would reduce visible call sites at the cost of creating an opaque law surface. That is the wrong trade for this fork.
