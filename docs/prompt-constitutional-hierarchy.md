# Prompt Constitutional Hierarchy

This document defines a prompt/runtime architecture for Codex based on four distinct layers:

- law
- posture
- assignment
- state

The goal is to stop these concerns from smearing into each other during normal execution,
sub-agent spawn, compaction, resume, and model swap.

## Core Model

| Layer | Semantic role | Typical examples | Allowed mutation |
| --- | --- | --- | --- |
| Constitutional | Law | safety boundaries, epistemic discipline, conflict-resolution rules, tool-use law | `immutable_during_execution` |
| Role | Posture | researcher posture, implementer posture, reviewer posture, orchestrator posture | `replace_on_role_transition` |
| Task | Assignment | exact worker contract, ordered obligations, checkpoints, deliverables, stop conditions | `replace_on_assignment` |
| Runtime | State | current user request, bridge facts, memory facts, blockers, available tools, live environment | `recompute_from_current_facts` |

In one sentence:

> lower layers narrow upper layers; they do not rewrite them.

## Interpretation Rule

The runtime should interpret prompt layers with the following legality rule:

1. Constitutional law defines invariant execution boundaries.
2. Role posture defines how a class of work should be carried out within that law.
3. Task assignment binds the exact local obligation within that posture.
4. Runtime state supplies current facts and currently admissible state.

When a lower layer appears to conflict with an upper layer:

- prefer the reading that makes the lower layer a lawful specialization
- declare a true conflict only when lawful specialization is impossible

This means:

- task packets must not widen role posture
- role defaults must not widen constitutional law
- runtime facts must not silently become normative permission

## Mutation Contract

The layer system needs explicit persistence law, not just vertical scope.

| Layer | Persistence law |
| --- | --- |
| Constitutional | inherit across spawn, resume, compaction, and model swap |
| Role | inherit across ordinary execution and compaction; replace only on role transition |
| Task | replace on new assignment; clear on task completion or explicit reassignment |
| Runtime | recompute from current facts each turn; preserve only explicit factual carry-over artifacts |

This gives the system a legality test:

- if compaction rewrites constitutional or role law, it is illegal
- if a child inherits parent runtime phenomenology instead of whitelisted facts, it is illegal
- if task-local convenience rewrites role posture, it is illegal

## Operational Rules

### Normal Turn

Normal execution should resolve in this order:

1. constitutional law
2. active role posture
3. active task assignment
4. current runtime state

The runtime layer should mostly contribute facts, not norms.

Good runtime content:

- available tools
- current environment and cwd
- active blockers
- thread memory
- continuation bridge
- current user message

Bad runtime content:

- silent redefinition of what counts as good work
- hidden permission widening
- task-local workflow norms masquerading as immutable law

### Child Spawn

Spawn should follow explicit inheritance rules:

1. inherit constitutional layer
2. inherit or replace role depending on spawn type
3. replace task layer with the child assignment
4. initialize fresh runtime
5. import only explicitly whitelisted parent runtime facts

The child should not inherit the parent's whole phenomenology. It should inherit only:

- relevant factual baton state
- explicit open obligations needed for the child task
- whitelisted environment facts

It should not inherit:

- stale local momentum
- parent-specific runtime noise
- accidental normative pressure from previous task state

### Compaction

Compaction preserves continuity of execution, not continuity of law.

Compaction should carry forward:

- current task state
- open obligations
- blockers
- accepted recent decisions
- continuation artifacts needed to resume execution
- durable factual memory

Compaction should not rewrite:

- constitutional law
- role posture

At most, upper-layer law may be preserved by stable reference, stable source, or exact carry-over.
It should not be re-authored as a lossy recap.

This prevents constitutional drift by recap.

### Resume And Model Swap

Resume and model swap should preserve hierarchy while refreshing runtime.

Rules:

1. constitutional layer remains stable unless an explicit maintenance phase changed it
2. role layer remains stable unless the resumed agent is entering a different role
3. task layer remains whatever assignment is still active for the thread
4. runtime is rebuilt from reconstructed history, current environment, and current model/runtime facts

Model change may require:

- refreshed runtime context
- refreshed model-specific instructions
- explicit warning when posture or behavior defaults may shift

Model change should not itself rewrite the constitution.

## Current Codex Mapping

In the current Codex stack, the rough mapping is:

- constitutional: `base_instructions`
- role: collaboration-mode instructions plus role-specific `developer_instructions`
- task: spawned worker assignment and local worker contract
- runtime: initial context, contextual environment bundle, history, `continuation_bridge`, `thread_memory`, live tool state

The main architectural risk is when `base_instructions` contain role-colored heuristics such as
generic exploratory posture. When that happens, child workers inherit those defaults as if they
were constitutional law, and lower layers are forced to compete with them rather than specialize
them.

That failure mode typically appears as:

- explicit task ordering says "checkpoint first"
- inherited generic posture says "explore/document first"
- worker tries to satisfy both
- orchestrator interprets the resulting exploration as drift or disobedience

This is not just a worker bug. It is a hierarchy bug.

## Refactor Direction For This Fork

The intended repo direction is:

1. make `base_instructions` truly constitutional
2. move exploration style, work-mode defaults, and narrative posture into role or collaboration-mode layers
3. make spawned-worker task packets explicit and phase-ordered
4. keep bridge and memory artifacts factual and local
5. preserve hierarchy during compaction instead of summarizing it into prose

That yields a stronger prompt model:

- law is stable
- posture is role-scoped
- assignment is explicit
- state is factual

This is more than prompt hygiene. It is a specialization calculus with mutation law.

## Maintenance Phase

The constitutional layer is intentionally immutable during ordinary execution.

It may only change during an explicit maintenance or constitutional-review phase.

That phase is separate from normal task execution and should be treated as a deliberate update to
the system's execution law, not as an incidental side effect of runtime summarization or task
completion.
