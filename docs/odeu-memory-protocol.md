# ODEU Memory Protocol v0

This document defines a minimal protocol for a local-compaction experiment where:

- `continuation_bridge` remains the short-horizon baton for immediate continuation
- a separate `thread_memory` JSON object becomes the durable semantic memory
- normal assistant turns emit small `odeu_delta` blocks instead of relying on transcript summarization
- `chill` turns are excluded from semantic memory updates

This is a protocol and merge-spec document, not an implementation note.

## Goals

- Preserve conceptual state rather than wording history.
- Supersede retained old transcript text with a durable semantic memory object.
- Make semantic ingress explicit and sparse.
- Let the user opt out of memory writes for casual or exploratory chat.
- Keep compaction incremental by updating the previous memory object rather than regenerating it from scratch.

## Non-goals

- Reconstructing the full transcript from memory.
- Preserving every user or assistant phrasing choice.
- Building a full semantic graph in v0.
- Making every assistant turn write to memory.

## Core Artifacts

### `continuation_bridge`

The existing `continuation_bridge` remains unchanged in role:

- short-horizon baton
- regenerated fresh on each compaction
- optimized for immediate continuation after compaction

### `thread_memory`

`thread_memory` is the durable semantic object stored in compaction replacement history.

It is authoritative for:

- active subjects
- accepted and revised claims
- active constraints and non-goals
- user goals, pain points, and success criteria
- current open loops and next expected moves

It is not authoritative for:

- exact transcript wording
- low-level step-by-step historical narration

### `odeu_delta`

`odeu_delta` is the semantic ingress format produced during normal assistant turns.

It is:

- concise
- incremental
- revision-aware
- scoped to meaningful conceptual changes

Compaction merges `odeu_delta` blocks into `thread_memory`.

## Turn Modes

Turn mode controls whether a turn is eligible to contribute semantic memory.

### User-facing tag

The user may annotate a turn with:

```text
<turn_mode>chill</turn_mode>
```

or:

```text
<turn_mode>work</turn_mode>
```

### Semantics

- `work`
  - default mode if no tag is present
  - assistant may emit `odeu_delta`
  - compaction updater may use the turn for memory updates

- `chill`
  - assistant must not emit `odeu_delta`
  - compaction updater must ignore the turn for semantic-memory purposes
  - any useful insight from a chill turn must be explicitly reintroduced later in a `work` turn through a new `odeu_delta`

### Determinism rule

Turn mode must be treated as explicit protocol, not inferred from tone or topic.

## `odeu_delta` Block Format

Assistant turns may emit one or more `odeu_delta` blocks when semantic state changes.

Recommended wire format:

```text
<odeu_delta id="D014" rev="2" supersedes="D014.r1" status="revised" subject="S3">
O:
- ...

E:
- accepted: ...
- refined: ...
- rejected: ...
- uncertain: ...

D:
- constraints: ...
- forbidden: ...

U:
- user_goals: ...
- success_criteria: ...
- pain_points: ...

Next:
- ...
</odeu_delta>
```

### Required attributes

- `id`
  - stable delta family identifier, for example `D014`
- `rev`
  - revision number within the family, starting at `0`
- `status`
  - one of:
    - `proposed`
    - `revised`
    - `settled`
    - `withdrawn`

### Optional attributes

- `supersedes`
  - previous concrete revision, for example `D014.r1`
- `subject`
  - stable subject identifier, for example `S3`
  - omit only when the delta updates thread-global state rather than a subject packet

### Delta family rules

- New conceptual item:
  - create a new family, for example `D018.r0`
- Correction or refinement:
  - keep the same family id
  - increment `rev`
  - set `supersedes`
- Explicit invalidation:
  - emit a new revision with `status="withdrawn"`

### Status meaning

- `proposed`
  - assistant believes the semantic update is useful, but it is not yet explicit user-ratified state
- `revised`
  - assistant reformulation after correction, refinement, or clearer framing
- `settled`
  - explicitly accepted by the user, or later relied upon as accepted thread state
- `withdrawn`
  - previous family line should no longer be used as active memory

## Delta Emission Rules

Assistant emits `odeu_delta` only when a turn changes semantic state.

Emit a delta when the turn:

- introduces a new claim worth remembering
- refines or corrects a previous claim
- records a decision
- adds or tightens a constraint
- changes user-goal understanding
- opens or closes an important loop
- changes the expected next move

Do not emit a delta when the turn is only:

- conversational filler
- stylistic reformulation
- tool-output narration without semantic consequence
- `chill` mode

### Anti-bloat rules

- Prefer one delta per subject per assistant turn.
- Keep each section to load-bearing points only.
- Do not restate unchanged ODEU state.
- Do not duplicate the same fact across `O`, `E`, `D`, `U`, and `Next`.

## User Corrections and Authority

User corrections outrank assistant deltas.

Operationally:

- if the user corrects a prior delta and the assistant emits a revised delta afterward:
  - the revised delta becomes the active candidate state
- if the user corrects a prior delta and no revised delta follows before compaction:
  - compaction must not preserve the contradicted delta as accepted
  - it must downgrade the family into uncertainty or pending correction

### Important distinction

No correction is not the same as explicit agreement.

Therefore:

- `settled` should be used only when there is clear ratification or later accepted dependence
- otherwise the latest family revision remains `proposed` or `revised`

## `thread_memory` Update Model

`thread_memory` is updated incrementally from the previous checkpoint.

Compaction must not regenerate a fresh global summary from the entire thread.

Instead it must:

1. load the previous `thread_memory` object
2. find all eligible `odeu_delta` blocks since the previous compaction
3. resolve revisions and user corrections
4. apply changes only to touched subjects and continuity state
5. emit a new `thread_memory` object

### Storage rule

For this experiment, older retained raw user/assistant text from previous compaction epochs is superseded by `thread_memory`.

After compaction, replacement history should prefer:

- initial context as needed by the compaction path
- one tiny summary or usage note
- `thread_memory`
- `continuation_bridge`
- required non-semantic runtime artifacts such as ghost snapshots

It should not preserve older raw transcript merely for historical recap.

## Merge Rules

### Input set

For a new compaction epoch:

- `previous_thread_memory`
- transcript items since the last compaction
- any `turn_mode` tags in those items
- all `odeu_delta` blocks in eligible assistant turns

### Eligibility filter

- Ignore all turns marked `chill`.
- Ignore assistant turns without `odeu_delta`.
- Ignore free-form transcript prose as memory input except for unresolved user corrections that have not yet been reflected in a later delta.

### Delta family resolution

For each `id`:

1. collect all revisions
2. sort by `rev`
3. verify monotonicity
4. keep the latest non-withdrawn revision as the active candidate
5. if the latest revision is `withdrawn`, remove or deactivate the family effect

If revision chain is malformed:

- keep the highest observed revision
- record an epistemic warning in memory rather than failing open

### User-correction override rule

If a user message after `D014.r2` contradicts `D014.r2` and no later `D014.r3` exists:

- `D014.r2` must not be treated as accepted
- its effect is downgraded to uncertainty or pending correction

### Subject application rule

Each active resolved family updates either:

- one subject packet, via `subject="Sx"`
- or thread-global ODEU / continuity state when `subject` is absent

Families must update only the minimal touched region of memory.

### Status application rule

- `proposed`
  - enters provisional memory state
- `revised`
  - supersedes prior family revision and remains provisional unless settled
- `settled`
  - enters accepted memory state
- `withdrawn`
  - removes the active effect of the family and may append a rejected-path or withdrawn-note marker

## Continuity Rules

Compaction must also update:

- open loops
- latest decisions
- active blockers
- next expected moves

These should be driven by resolved `odeu_delta` families, not by free-form recap.

## Suggested Minimal `thread_memory` Responsibilities

The exact JSON schema can evolve, but v0 memory should at least preserve:

- thread-global ODEU
- subject inventory
- accepted and provisional claims
- active constraints
- user goals and pain points
- open loops
- latest decisions
- next expected moves

## Minimal Usage Summary

Alongside the JSON artifacts, compaction may keep a tiny usage note like:

```text
Use `thread_memory` as authoritative long-horizon semantic state.
Use `continuation_bridge` as the immediate baton for resuming the current task.
Ignore older transcript history from prior compaction epochs.
```

This note is explanatory only. It must not carry substantive memory content already present in the JSON artifacts.

## Worked Example

1. Assistant emits:

```text
<odeu_delta id="D007" rev="0" status="proposed" subject="S2">
E:
- accepted: remote continuation bridge should remain separate from long-horizon memory
</odeu_delta>
```

2. User corrects:

```text
No, use local compaction for the memory experiment.
```

3. Assistant emits:

```text
<odeu_delta id="D007" rev="1" supersedes="D007.r0" status="revised" subject="S2">
O:
- long-horizon memory experiment should run on the local compaction path

E:
- accepted: continuation bridge remains separate
- accepted: local compaction is the intended experiment path
</odeu_delta>
```

4. Compaction keeps `D007.r1` as the active family revision and discards the semantic effect of `D007.r0`.

## Implementation Assumption

This protocol is intended for a force-local compaction experiment.

If later remote compaction is used on the same thread family, remote post-processing must explicitly preserve or reinsert `thread_memory`, otherwise the durable memory object will be lost from replacement history.
