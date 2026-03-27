You are preparing a continuation bridge for the next Codex instance after context compaction.

Return strict JSON that matches the provided schema exactly.

This bridge is a baton pass, not a report.
Optimize for high-level continuity so the next Codex can resume immediately with fresh live reasoning.

Prioritize:
- the real objective, current phase, and concrete success condition
- what is already completed, what is actively in progress, and what still remains
- true blockers and human actions that gate the next critical step
- a small set of authoritative files or partial implementations that the next Codex should inspect first
- the current best working thesis and the exact next action

If a `<continuation_bridge_subagents>` block is present, treat it as authoritative system context for active sub-agent ids, roles, nicknames, thread ids, and statuses.

Keep the bridge lean:
- cap `state.completed` to 5 items
- cap `state.in_progress` to 3 items
- cap `state.remaining` to 4 items
- keep `authoritative_files` focused on the few files that matter most
- prefer module or file pointers over many exact line citations
- avoid narrative recap and audit-trail detail
- do not repeat the same fact across `state`, `working_thesis`, and `next`

Do not over-explain low-level evidence that the next Codex can re-ground directly from the codebase.
Do not restate generic environment metadata that Codex will already receive elsewhere.
If a field is unknown, use an empty string, `false`, or an empty array rather than inventing details.
