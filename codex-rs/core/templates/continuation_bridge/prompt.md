You are preparing a continuation bridge for the next Codex instance after context compaction.

Return strict JSON that matches the provided schema exactly.

Summarize only durable, task-relevant state from the current thread:
- capture the real objective, current phase, and concrete success condition
- record repo identity so the recipient does not have to re-check cwd, branch, head, or worktree dirtiness
- record work already completed, partial work in flight, and work not started
- separate true blockers from non-blocking issues and optional follow-ups
- if sub-agents exist, record each one as structured state with role, thread id, status, and whether it blocks the main flow
- prefer exact file paths, symbols, commands, and decisions over vague prose
- for analytical tasks, include 3-7 key claims with exact file/line evidence when available
- every `key_claims_with_evidence.evidence` entry must include `path`, `line`, `kind`, and `why_it_supports_claim`
- prefer `code`, `test`, `doc`, or `config` for `kind`; use an empty string only if none fit
- record the current best thesis and likely conclusion, even if they remain provisional
- keep recommended_output_shape to concrete section labels or deliverable components, not prose instructions
- list invariants, constraints, and assumptions that must carry forward
- capture resolved questions, remaining uncertainties, and rejected paths
- make the next action concrete enough that a fresh Codex can resume immediately
- prefer evidence-backed claims over narrative recap
- do not mark something as blocking unless the next critical action truly depends on it

Do not repeat generic environment metadata that Codex will already receive elsewhere.
If a field is unknown, use an empty string, `false`, or an empty array rather than inventing details.
