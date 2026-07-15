# Semantic Broker Tool-Surface Alignment Spec

This spec defines the follow-up fix for the `0.125` audit finding that the
semantic broker can describe a different tool surface than the one actually sent
to the model.

Primary references:

- [semantic-broker-implementation-spec.md](/home/rose/work/codex/fork/docs/fork/semantic-broker-implementation-spec.md)
- [semantic-broker-v0-hardening-implementation-spec.md](/home/rose/work/codex/fork/docs/fork/semantic-broker-v0-hardening-implementation-spec.md)
- [release-0.125-alignment-implementation-spec.md](/home/rose/work/codex/fork/docs/fork/release-0.125-alignment-implementation-spec.md)

## Judgment

This finding strengthens the adapter between vanilla core and the fork-owned
semantic broker. The broker crate can remain structurally correct while still
receiving an inaccurate runtime view if core computes broker-visible tools by a
different path than `build_prompt(...)`.

The right fix is not to move prompt construction into `codex-semantic-broker`.
The right fix is to make vanilla prompt-visible tool filtering a single
core-owned helper and have both prompt construction and broker input construction
use that helper.

## Problem

`build_prompt(...)` filters deferred dynamic tools using full `ToolName`
identity:

- plain tools are matched as `ToolName::plain(name)`.
- namespaced tools are matched as `ToolName::namespaced(namespace, name)`.
- a namespace spec is removed when all of its child functions are deferred.

`semantic_broker_runtime::visible_tool_names_for_prompt(...)` currently
recomputes the view using only `tool.name.as_str()` against `ToolSpec::name()`.
That loses namespace identity and can leave a namespace listed in broker packet
tool bindings even when the actual prompt tool list has removed that namespace.

The immediate runtime impact is low because broker v0 uses tool bindings as
metadata. The boundary risk is real: the active context packet should describe
the exact prompt surface that accompanies it, especially before future schema
selection starts depending on tool availability.

## Scope

In scope:

1. Share prompt-visible tool filtering between `build_prompt(...)` and semantic
   broker input construction.
2. Preserve current Responses API prompt behavior.
3. Preserve semantic broker v0 packet shape: `tool_bindings: Vec<String>`.
4. Add regression coverage for deferred namespaced dynamic tools.

Out of scope:

- changing dynamic tool registration or `tool_search` behavior.
- changing the broker crate to depend on `codex-core`.
- expanding broker packet tool bindings to structured namespace/function pairs.
- enabling `Feature::SemanticBroker` by default.
- recording broker packets in durable history.
- broad governance or prompt-layering refactors.
- remote compact prompt construction, unless tests show it should share the
  same deferred dynamic-tool filtering in this PR. This PR aligns broker input
  with ordinary sampling prompts first.

## Required Invariants

1. `codex-semantic-broker` remains the owner of broker registry, candidate,
   adjudication, packet, and render law.
2. `core` remains the runtime adapter and prompt-surface owner.
3. The broker packet must not list a tool or namespace that is absent from the
   actual `Prompt.tools` list for the same sampling request.
4. Deferred dynamic tools remain callable by runtime/search flows where intended,
   but absent from ordinary model-visible prompt tools.
5. Namespace filtering must use full `ToolName` identity, not plain string names.
6. The fix must not add a dependency from `codex-semantic-broker` to `codex-core`
   or `codex-tools`.

## Target Design

Add a small core-owned prompt-surface helper under the existing tools area:

```text
codex-rs/core/src/tools/prompt_surface.rs
```

Recommended API shape:

```rust
pub(crate) fn filter_deferred_dynamic_tool_specs(
    specs: Vec<ToolSpec>,
    dynamic_tools: &[DynamicToolSpec],
) -> Vec<ToolSpec>;

pub(crate) fn model_visible_specs_for_prompt(
    router: &ToolRouter,
    turn_context: &TurnContext,
) -> Vec<ToolSpec>;

pub(crate) fn model_visible_tool_names_for_prompt(
    router: &ToolRouter,
    turn_context: &TurnContext,
) -> Vec<String>;
```

`filter_deferred_dynamic_tool_specs(...)` is the lower-level pure helper. Unit
tests should target this helper directly so they do not need to construct a full
`ToolRouter` and `TurnContext`.

`model_visible_specs_for_prompt(...)` should move the current filtering logic out
of `session/turn.rs::build_prompt(...)` without changing behavior:

- build `HashSet<ToolName>` from `dynamic_tools` where `defer_loading` is true,
  using `ToolName::new(tool.namespace.clone(), tool.name.clone())` as the single
  source of deferred dynamic-tool identity;
- return `router.model_visible_specs()` unchanged when the set is empty;
- for `ToolSpec::Function`, drop only matching plain deferred tools;
- for `ToolSpec::Namespace`, retain only child functions whose namespaced
  `ToolName` is not deferred;
- drop an empty namespace after child filtering;
- preserve all other `ToolSpec` variants unchanged.

`model_visible_tool_names_for_prompt(...)` should derive broker v0 names from the
already-filtered specs:

- map `ToolSpec::name()` after filtering;
- sort for stable packet output;
- deduplicate defensively.

For v0, a namespace contributes one binding by namespace name if and only if the
filtered prompt still includes that namespace spec. The broker does not need
child-level tool bindings in this slice.

Do not reintroduce string-set deferred filtering in
`semantic_broker_runtime.rs`.

Recommended implementation skeleton:

```rust
pub(crate) fn model_visible_specs_for_prompt(
    router: &ToolRouter,
    turn_context: &TurnContext,
) -> Vec<ToolSpec> {
    filter_deferred_dynamic_tool_specs(
        router.model_visible_specs(),
        &turn_context.dynamic_tools,
    )
}

pub(crate) fn model_visible_tool_names_for_prompt(
    router: &ToolRouter,
    turn_context: &TurnContext,
) -> Vec<String> {
    let mut names = model_visible_specs_for_prompt(router, turn_context)
        .into_iter()
        .map(|spec| spec.name().to_string())
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}

pub(crate) fn filter_deferred_dynamic_tool_specs(
    specs: Vec<ToolSpec>,
    dynamic_tools: &[DynamicToolSpec],
) -> Vec<ToolSpec> {
    let deferred_dynamic_tools = dynamic_tools
        .iter()
        .filter(|tool| tool.defer_loading)
        .map(|tool| ToolName::new(tool.namespace.clone(), tool.name.clone()))
        .collect::<HashSet<_>>();

    if deferred_dynamic_tools.is_empty() {
        return specs;
    }

    specs
        .into_iter()
        .filter_map(|spec| {
            filter_deferred_dynamic_tool_spec(spec, &deferred_dynamic_tools)
        })
        .collect()
}
```

## Work Packages

### TS1: Extract Shared Prompt-Surface Helper

Files:

- [mod.rs](/home/rose/work/codex/fork/codex-rs/core/src/tools/mod.rs)
- new `codex-rs/core/src/tools/prompt_surface.rs`
- [turn.rs](/home/rose/work/codex/fork/codex-rs/core/src/session/turn.rs)

Steps:

1. Move `filter_deferred_dynamic_tool_spec(...)` out of `turn.rs`.
2. Add `filter_deferred_dynamic_tool_specs(...)` in
   `tools/prompt_surface.rs`.
3. Add `model_visible_specs_for_prompt(...)` as the production wrapper over
   `router.model_visible_specs()` plus the pure filter helper.
4. Update `build_prompt(...)` to call the helper instead of locally rebuilding
   the filtered tool list.
5. Keep `build_prompt(...)` output byte-for-byte equivalent for existing cases.

Acceptance criteria:

- existing prompt construction tests continue to pass.
- no semantic broker code is required to preserve vanilla prompt behavior.
- remote compact prompt construction remains unchanged in this PR unless a test
  exposes a direct mismatch that must share the new helper.

### TS2: Use Shared Prompt Surface for Broker Input

Files:

- [semantic_broker_runtime.rs](/home/rose/work/codex/fork/codex-rs/core/src/semantic_broker_runtime.rs)
- [turn_tests.rs](/home/rose/work/codex/fork/codex-rs/core/src/session/turn_tests.rs)

Steps:

1. Remove the local plain-name deferred filtering from
   `visible_tool_names_for_prompt(...)`.
2. Build broker `visible_tool_names` from
   `tools::prompt_surface::model_visible_tool_names_for_prompt(...)`.
3. Keep `BrokerInput.visible_tool_names` as `Vec<String>` for this slice.
4. Do not add packet-law parsing or tag/schema literals to core tests. Use
   broker-owned helpers such as `is_active_context_packet_text(...)` for packet
   recognition. If a test truly needs packet payload extraction, add a
   broker-owned test/debug helper rather than re-parsing the active packet
   contract in core.

Acceptance criteria:

- broker packet `tool_bindings` are a projection of the actual filtered
  `Prompt.tools`, not a separately approximated surface.
- the broker runtime adapter still does not score schemas, choose operators, or
  invent broker packet fields.

### TS3: Regression Tests for Namespaced Deferred Dynamic Tools

Required tests:

1. Unit-level prompt-surface test:
   - a plain dynamic function is removed when deferred.
   - a namespace with only deferred dynamic child tools is removed.
   - a namespace with at least one non-deferred child remains.
   - non-dynamic specs remain unchanged.

2. Broker adapter test:
   - enable `Feature::SemanticBroker`;
   - configure a namespaced deferred dynamic tool that would otherwise create a
     namespace-only prompt spec;
   - build the router and broker overlay;
   - build the prompt from the same router/context;
   - assert one broker-owned prompt-only packet exists;
   - assert the deferred namespace is absent from the helper-derived broker
     names and from `prompt.tools`;
   - assert `tool_search` remains present if the configured tool surface keeps
     it visible for deferred dynamic-tool discovery.

3. Prompt parity test:
   - compare ordered specs:
     `model_visible_specs_for_prompt(router, turn_context) ==
     build_prompt(...).tools`;
   - separately assert that `model_visible_tool_names_for_prompt(...)` returns
     sorted, deduplicated names.

Useful test names:

```text
prompt_surface_drops_plain_dynamic_function_when_deferred
prompt_surface_drops_namespace_when_all_dynamic_children_deferred
prompt_surface_retains_namespace_with_visible_child_after_filtering
prompt_surface_keeps_non_dynamic_specs_unchanged
semantic_broker_tool_bindings_match_prompt_visible_tools_with_deferred_namespace
```

## PR Shape

Single focused PR:

```text
SB-TS1: Share prompt-visible tool filtering with semantic broker
```

Expected files:

```text
codex-rs/core/src/tools/mod.rs
codex-rs/core/src/tools/prompt_surface.rs
codex-rs/core/src/session/turn.rs
codex-rs/core/src/semantic_broker_runtime.rs
codex-rs/core/src/session/turn_tests.rs
```

Do not combine with:

- observability wildcard/no-op classification;
- batched current-turn broker input;
- thread-memory governance naming;
- governance context assembly cleanup.

## Validation Plan

Minimum validation:

```bash
cd codex-rs
just fmt
cargo test -p codex-core semantic_broker_tool_bindings_match_prompt_visible_tools_with_deferred_namespace --lib
cargo test -p codex-core prompt_surface_ --lib
```

Adjacent validation:

```bash
cd codex-rs
cargo test -p codex-core semantic_broker_ --lib
cargo test -p codex-core search_tool --test all
cargo test -p codex-tools
```

If `prompt_surface_` does not match the final test module path, run the exact new
test names instead.

## Review Checklist

- PR diff contains only fork-specific adapter hardening, no inherited upstream
  commits.
- `build_prompt(...)` and broker input construction call the same prompt-surface
  helper.
- no broker crate dependency changes.
- no durable history changes.
- no changes to dynamic tool handler registration.
- namespace-only deferred dynamic tools are absent from both `Prompt.tools` and
  broker packet `tool_bindings`.

## Deferred Follow-Ups

These are related but intentionally separate:

- represent broker tool bindings as structured namespace/function pairs if a
  future broker version needs child-level tool reasoning.
- preserve batched pending user-turn text instead of a single overwritten string.
- explicitly classify remaining observability-adjacent `0.125` events.
- decide whether `ThreadMemoryGovernance::Disabled` should be renamed or changed
  behaviorally.
