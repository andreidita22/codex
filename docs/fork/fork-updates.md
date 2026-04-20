# Fork Update Workflow

This fork is no longer maintained as a tiny bolt-on branch rebased directly on
top of upstream release tags.

The fork's `main` branch is now the canonical architecture. New upstream Codex
releases are treated as inputs to ingest into this fork, not as the branch we
rebase the fork away onto.

Use this document together with:

- [fork-update-checklist.md](fork-update-checklist.md)
- [custom-fork-module-inventory.md](custom-fork-module-inventory.md)
- [release-alignment-log.md](release-alignment-log.md)

Those documents answer:

- the short operational release-ingest sequence
- what custom modules the fork owns
- which shared upstream seam files those modules depend on
- what alignment decisions were made on previous release ingests

This document answers a different question:

- what exact branch workflow should be used when a new upstream release lands

## Core rule

Always start a new release ingest from the current fork `main`, not from the
upstream release branch.

If you prepare the ingest branch from upstream instead, GitHub will later see
the branch as missing the already-merged fork history on `main`. That produces
synthetic PR conflicts in fork-owned files even when the actual code alignment
is already correct.

## Branch roles

- `main`
  - authoritative fork branch
  - contains both upstream functionality already ingested and the fork-owned
    custom modules
- `upstream-main`
  - read-only local tracking branch for the latest upstream `rust-v*` release
  - used for comparison and alignment, not as the base of the ingest branch
- `codex/update-<version>-ingest`
  - temporary local-only branch created from current fork `main`
  - used to ingest the new upstream release into the fork baseline
  - not opened as a PR
- `codex/update-<version>-align`
  - temporary review branch created from the newly updated fork `main`
  - contains only fork-owned adaptation commits made after the upstream ingest
  - this is the branch that should be opened as a PR

## One-time setup

```sh
git config --local rerere.enabled true
git config --local rerere.autoupdate true
```

That lets Git reuse repeated seam resolutions across future release ingests.

## Recommended release ingest workflow

### 1. Update the upstream tracking branch

Fetch upstream tags and move `upstream-main` to the new release tag.

```sh
git fetch upstream --tags --prune
git branch -f upstream-main rust-v0.120.0
```

You can verify the result with:

```sh
git rev-parse upstream-main
git log --oneline -1 upstream-main
```

### 2. Start the ingest branch from fork `main`

Create the ingest branch from current fork `main`, not from `upstream-main`.

```sh
git switch main
git pull --ff-only origin main
git switch -c codex/update-0.120-ingest
```

This branch is for local upstream ingestion only. It is not the review branch.

### 3. Record the release in the alignment log before editing

Add a new section to [release-alignment-log.md](release-alignment-log.md)
covering:

- current fork `main`
- target upstream release
- comparison range
- likely seam files
- initial risk read

Do this before resolving files so the decisions are captured as they are made,
not reconstructed later.

### 4. Compare the new upstream release against the fork

Use `upstream-main` as the comparison target and identify shared seam files
before touching code.

Typical commands:

```sh
git diff --stat main..upstream-main
git diff --name-only main..upstream-main
git diff --name-only main...upstream-main
```

The most important files are usually the shared seam files already tracked in
[custom-fork-module-inventory.md](custom-fork-module-inventory.md) and prior
entries in [release-alignment-log.md](release-alignment-log.md).

### 5. Align upstream changes into the ingest branch

Resolve the upstream delta from inside the branch that was created from fork
`main`.

The default decision on shared seam files is:

- `merge_both`

Only choose `accept_upstream` or `keep_fork` when the reason is explicit and
recorded in the release log.

This usually means:

- preserving fork-owned modules as authoritative
- re-aligning the shared seam files where upstream moved surrounding structure
- keeping upstream improvements where they do not violate fork invariants

At this stage, prefer mechanical release ingestion and seam reconciliation only.
Do not bundle extra fork feature work here if it can be separated into a later
alignment branch.

### 6. Validate the aligned branch

Run the targeted validation required by the touched seam files. Common examples:

```sh
cd codex-rs
just write-config-schema
cargo test -p codex-config
cargo test -p codex-tools
cargo test -p codex-core tools::spec::tests::
just fix -p codex-config
just fix -p codex-tools
just fix -p codex-core
just fmt
./tools/argument-comment-lint/run.py
```

If dependencies changed:

```sh
cd ..
just bazel-lock-update
just bazel-lock-check
```

Also run any release-specific manual checks needed for fork-owned behavior, for
example:

- continuation bridge generation and reinsertion
- thread-memory generation and fail-closed behavior
- governance prompt-layer insertion
- E-witness tool exposure
- thread-spawn tool containment

### 7. Land the ingest step locally, without a PR

Once the upstream ingest branch is validated, land it onto fork `main`
directly. This keeps the upstream release delta out of PR review and prevents
bot reviews from wasting effort on upstream commits.

Typical pattern:

```sh
git switch main
git merge --ff-only codex/update-0.120-ingest
git push origin main
```

If a fast-forward is not possible because you intentionally used a temporary
integration branch with local-only commits, merge or swap intentionally only
after validation.

### 8. Create the PR branch for fork-owned alignment commits

Now branch from the newly updated fork `main` and place only fork-specific
adaptation commits there.

```sh
git switch main
git pull --ff-only origin main
git switch -c codex/update-0.120-align
```

This branch should contain only:

- fork-owned follow-up adjustments
- reviewable policy/alignment choices
- any fixes discovered only after the upstream ingest baseline was landed

Open the PR from `codex/update-0.120-align` into fork `main`.

This keeps bot review focused on the fork delta instead of the upstream release
churn.

## Mistake to avoid

Do not treat upstream release branches as the base of truth once the fork has
its own substantial architecture.

The broken pattern looks like this:

1. create the release branch from upstream `rust-v*`
2. reapply or rebase fork changes there
3. open a PR into fork `main`

That causes GitHub to compare:

- fork `main` with full existing fork history
- against an upstream-based branch that does not contain that same history

The result is a false conflict surface in fork-owned files and shared seam files.

The correct pattern is:

1. update `upstream-main`
2. branch from current fork `main`
3. ingest upstream changes there
4. validate
5. land the ingest onto `main` locally, without a PR
6. branch again from the updated `main`
7. commit only fork-owned alignment work
8. open the PR only for that fork-owned alignment branch

## Legacy note

The helper script:

```sh
scripts/rebase-bolt-on-release.sh
```

belongs to the older maintenance model where the customization lived as a small
linear patch on top of upstream releases. It should not be used for updating the
fork `main` branch now that the fork itself is the canonical source.
