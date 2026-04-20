# Agent Observability Implementation Spec

This is the family-level implementation spec for the E-witness / sub-agent observability extraction described in [agent-observability-extraction-plan.md](/home/rose/work/codex/fork/docs/agent-observability-extraction-plan.md).

The worked-example reference pattern is the context-maintenance extraction/hardening family.

## Family Invariant

Observability law should be owned by one semantic module.

That means:

- event reduction and snapshot meaning belong to the observability crate
- model-visible observability tool names and enum/string domains must come from that owner
- runtime/session/tool-registration code may translate and execute, but must not become a second law surface

Protocol-owned lifecycle status is allowed in the new crate.

That means:

- `codex-agent-observability` may consume protocol `AgentStatus`
- `codex-core` should retrieve live lifecycle status
- `codex-agent-observability` should decide how that status affects finality, phase semantics, and snapshot output

Thread-spawn collaboration-surface policy is intentionally separate:

- it stays in `codex-tools`
- it must still become explicit and typed

## PR A0 — Behavior Locks

### Purpose

Freeze current behavior before the extraction move starts.

### Required locks

- this PR locks current behavior only; it does not assume the final semantic owner already exists
- progress phase schema matches the runtime phase set
- thread-spawned sub-agents do not receive `spawn_agent`
- all sub-agents still do not receive `request_user_input`
- thread-spawned sub-agents still receive inspect/wait progress tools
- meaningful events advance sequence; non-meaningful events do not
- `TurnStarted` does not bump sequence
- repeated delta/heartbeat-style updates do not bump sequence
- warning / stream-error updates do not bump sequence
- terminal progress state remains stable
- initial phase match returns `already_satisfied`
- later phase match returns `phase_matched`
- `seq_advanced` takes precedence over phase matching

### Test layers

Module-law locks:

- temporary, near the current progress implementation

Tool-planning locks:

- `codex-tools`

Runtime wiring locks:

- `codex-core`

### Scope guard

This PR should not introduce a new crate yet.

## PR A1 — Crate Creation And Copy-Free Semantic Move

### Purpose

Create `codex-agent-observability` and move the live reducer-centered law into it without leaving duplicate doctrine behind.

### New crate

- `codex-rs/agent-observability`

### Required workspace wiring

- add `"agent-observability"` to workspace members
- add `codex-agent-observability = { path = "agent-observability" }` to workspace dependencies
- add a dependency in `core/Cargo.toml`
- do not introduce any reverse dependency from the new crate to `codex-core`

### Initial public surface

The initial public API should stay narrow and typed. It likely includes:

- progress-phase enum
- blocker/active-work enums and structs
- snapshot type
- registry type
- reducer entrypoints
- wait-classification entrypoints
- tool-contract constants or placeholders only if they are needed immediately

### Production ownership after this PR

Owned by `codex-agent-observability`:

- semantic progress types
- reducer logic
- registry behavior
- semantic interpretation of protocol lifecycle status
- wait-classification semantics needed by the live path

Still owned by `codex-core`:

- lifecycle seeding/removal
- target resolution
- handler execution
- session event hook

### Test layers

- crate contract tests move into the new crate
- runtime adapter tests remain in core

### Scope guard

Keep live behavior unchanged.

There must not be two live reducers claiming doctrine after this PR lands.

## PR A2 — Wait Semantics And Snapshot API Hardening

### Purpose

Finish the adapter-facing seam after reducer ownership moved.

### Required adapter updates

- `AgentControl` calls the observability crate for seed/record/inspect behavior
- `ThreadManager` stores the observability-owned registry type
- progress handlers consume observability-owned snapshots and wait classification
- session event flow records through the adapter without embedding reducer doctrine locally

### Wait semantics rule

The crate API must preserve the current distinction between:

- initial `already_satisfied`
- later `phase_matched`
- `seq_advanced`
- timeout

It must also preserve current precedence:

- `seq_advanced` wins before phase matching

Avoid an API that can only infer "matched" without knowing whether the observation was initial or later.

### Snapshot consumption rule

The PR must make external snapshot consumption explicit:

- either narrow accessor methods such as `seq()` and `phase()`
- or intentionally public fields, if that tradeoff is chosen explicitly

Fallback/not-found snapshot construction must also be explicit.

### Explicit boundary rule

`codex-core` may translate runtime status and event context, but it must not re-author:

- phase transitions
- snapshot meaning
- wait-match semantics
- progress sequence law

### Test layers

- core adapter/runtime tests
- existing behavior locks remain green

## PR A3 — Single-Source Tool Contract

### Purpose

Remove schema/name drift risk by making the observability crate the canonical owner of the progress tool contract constants.

### Canonical items

- `inspect_agent_progress` tool name
- `wait_for_agent_progress` tool name
- progress-phase string list used by schema generation
- blocker-reason string list used by schema generation
- active-work-kind string list used by schema generation
- wait outcome or match-reason string list used by schema generation

### Expected integration

Add a dependency in `tools/Cargo.toml` once this PR lands.

`codex-tools` should depend on `codex-agent-observability` for these contract constants.

`codex-core` should consume the same constants anywhere model-visible routing still needs them.

### Test layers

- contract tests in the new crate
- schema-facing tests in `codex-tools`
- minimal core router/wiring tests only where needed

### Scope guard

Do not move actual `ToolSpec` construction into the new crate. That remains in `codex-tools`.

## PR A4 — Explicit Tool-Surface Policy In `codex-tools`

### Purpose

Make thread-spawn containment readable and policy-shaped instead of implicit booleans.

### New shape

Introduce a typed planning object in `codex-tools`, for example:

- `AgentToolSurfacePolicy`

It should be derived from:

- session source
- collab-tool feature state
- any existing tool-surface feature gates already relevant to this path

### Policy should cover

- progress observability tools
- spawn-agent availability
- request-user-input availability
- any other current collaboration-surface inclusion rule already embedded in booleans/matches

### Boundary rule

This policy belongs in `codex-tools`, not in `codex-agent-observability`.

### Test layers

- `tool_config` and `tool_registry_plan` tests in `codex-tools`
- no new doctrine duplication in `codex-core`

## PR A5 — Cleanup And Narrowing

### Purpose

Finish the family by reducing leftover second-doctrine tests and overbroad exports.

### Expected cleanup

- move planning doctrine out of core tests
- keep core tests focused on handler mapping and runtime behavior
- narrow observability-crate public exports to the actual seam

### Scope guard

This PR should not introduce semantic changes. It is cleanup after ownership is stable.

## Cross-Cutting Rules

### No generic hub

Do not introduce a generic fork broker for this work.

The right shape is:

- hot upstream files
- thin adapters
- domain-specific module ownership

### No core dependency in the new crate

`codex-agent-observability` must not depend on `codex-core`.

It may depend on protocol-owned types that are currently re-exported by core.

### No runtime services in the new crate

Do not move:

- `Session`
- `TurnContext`
- thread lifecycle orchestration
- handler timeout loops
- tool registry planning

into the new crate.

### Keep `codex-tools` as the tool-planning owner

This family is not a reason to move tool registration out of `codex-tools`.

## Completion Criteria

This family is ready to stop when:

- the reducer and snapshot law live in `codex-agent-observability`
- observability-owned model-visible enum/string contracts are single-sourced
- thread-spawn containment is explicit in `codex-tools`
- core acts as adapter/runtime wiring instead of semantic owner
- doctrine tests primarily live with the owning crate
