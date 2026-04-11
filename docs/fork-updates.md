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
- `codex/update-<version>`
  - temporary ingest branch created from current fork `main`
  - used to align the fork to a new upstream release before merging or swapping

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
git switch -c codex/update-0.120
```

This preserves the correct ancestry for a later PR into fork `main`.

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

### 7. Merge or swap intentionally

If the ingest branch was created from `main`, a normal PR into fork `main`
should now have the correct ancestry and should not show synthetic conflicts.

If you ever end up with a validated branch that is intended to supersede `main`
but was prepared from the wrong ancestry, do not hand-resolve the synthetic PR
conflicts in GitHub. Use one of these two fixes instead:

1. Rebuild the ingest branch from current fork `main` and replay the alignment.
2. If the branch is already fully validated and meant to replace `main`, back
   up the old `main` and intentionally swap `main` to the validated branch.

Backup pattern:

```sh
git push origin origin/main:refs/heads/backup/main-before-0.120-swap-$(date +%Y%m%d)
```

Intentional swap pattern:

```sh
git branch -f main codex/update-0.120
git push origin codex/update-0.120:main --force-with-lease
git switch main
```

Only do this when the branch has already been validated as the new canonical
fork state.

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
5. merge or intentionally swap

## Legacy note

The helper script:

```sh
scripts/rebase-bolt-on-release.sh
```

belongs to the older maintenance model where the customization lived as a small
linear patch on top of upstream releases. It should not be used for updating the
fork `main` branch now that the fork itself is the canonical source.
