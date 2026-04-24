# Fork Update Checklist

Use this as the fast refresh for release ingests. For rationale and edge cases,
see [fork-updates.md](fork-updates.md) and
[upstream-ingest-watchlist.md](upstream-ingest-watchlist.md).

## Canonical rule

- Treat fork `main` as canonical.
- Treat `upstream-latest-release` as the read-only mirror of the target
  upstream `rust-v*` release for the current ingest. It may intentionally lag a
  newer upstream release while we ingest a specific version.
- Treat `upstream-main` as a separate read-only mirror of live upstream `main`
  for inspection only.
- Start every release ingest branch from current fork `main`, never from
  `upstream-latest-release` or `upstream-main`.
- Do not open a review PR from the raw upstream-ingest branch to `main`.
- Open review PRs only for the later fork-alignment branch that contains our
  commits.
- Never open or retarget an alignment PR to `main` until `main` already
  contains the ingest branch ancestry.

## Bot Review Topology

There are two valid review paths. Pick one before creating the alignment branch
and do not mix them.

### Path A: Review before landing ingest

Use this when bots or humans should review fork alignment commits before the
raw ingest branch lands in `main`.

#### 1. Build ingest ancestry first

1. Create `codex/update-<ver>-ingest` from current fork `main`.
2. Merge the upstream `rust-v*` release into `codex/update-<ver>-ingest`.
3. Resolve conflicts and run ingest validation there.

#### 2. Stack an align branch on top

1. Create `codex/update-<ver>-align` from `codex/update-<ver>-ingest`.
2. Add only fork-specific alignment commits on this branch.

#### 3. Open review PR against ingest branch

1. Open PR: `codex/update-<ver>-align` -> `codex/update-<ver>-ingest`.
2. Collect bot and human review there.
3. Resolve comments only on `codex/update-<ver>-align`.

Result: bots review only the fork alignment delta because base already includes
the upstream release ancestry.

#### 4. Land in main without losing ancestry

1. Merge `codex/update-<ver>-ingest` into `main` with ancestry preserved.
2. Do not squash this merge.

Then either:

1. Retarget the existing align PR to `main`, or
2. Open a new PR from `codex/update-<ver>-align` to `main`.

Because `main` now contains ingest ancestry, the PR diff stays alignment-only.

#### 5. Guard before retargeting to main

Before opening or retargeting `codex/update-<ver>-align` to `main`, run:

```sh
git merge-base --is-ancestor codex/update-<ver>-ingest main
git log --oneline main..codex/update-<ver>-align
```

The first command must pass. The second command must show only fork-owned
alignment commits, not upstream release commits.

### Path B: Land ingest first, then review

Use this when the raw ingest branch has already been validated and can be
landed before bot review starts.

1. Merge `codex/update-<ver>-ingest` into `main` with ancestry preserved.
2. Create `codex/update-<ver>-align` from the updated `main`.
3. Add only fork-specific alignment commits.
4. Open PR: `codex/update-<ver>-align` -> `main`.

### Failure mode to avoid

If you open `codex/update-<ver>-align` directly against `main` before ingest
ancestry is in `main`, GitHub and bots will include upstream release commits in
review scope.

## Standard sequence

### 1. Move the upstream mirrors

```sh
git fetch upstream --tags --prune
git branch -f upstream-latest-release rust-v<ver>
git branch -f upstream-main upstream/main
```

Pushing mirror branches to `origin` is optional and may be blocked by repository
policy. The local mirror branches are the required inputs for ingest prep.

### 2. Start from fork `main`

```sh
git switch main
git pull --ff-only origin main
git switch -c codex/update-<ver>-ingest
```

### 3. Seed the decision log before editing

- Add a new release section to [release-alignment-log.md](release-alignment-log.md).
- Record:
  - fork `main` SHA
  - upstream target SHA
  - likely seam files
  - initial risk read

### 4. Compare fork vs upstream

```sh
git diff --stat <old-release-tag>..upstream-latest-release
git diff --name-only <old-release-tag>..upstream-latest-release
git diff --stat main..upstream-latest-release
git diff --name-only main..upstream-latest-release
git diff --name-only main...upstream-latest-release
```

- Use [custom-fork-module-inventory.md](custom-fork-module-inventory.md) to
  prioritize seam files.
- Run the grep gates and enum review in
  [upstream-ingest-watchlist.md](upstream-ingest-watchlist.md).
- Record likely bypass-path and ownership-regression risks in the release log.
- Use `upstream-main` only when you want to inspect live upstream changes that
  are not yet part of the tagged release.

### 5. Align into the ingest branch

- Default shared seam decision: `merge_both`
- Only use `accept_upstream` or `keep_fork` with an explicit rationale in the
  release log.

### 6. Validate

Typical checks:

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

Required validation order:

1. semantic-owner tests first
2. then high-signal adapter/runtime tests
3. manually review any compaction snapshot churn

### 7. Land or review with the selected topology

If using Path A, create the alignment branch before landing ingest and open the
first review PR against the ingest branch:

```sh
git switch codex/update-<ver>-ingest
git switch -c codex/update-<ver>-align
# add only fork-specific alignment commits
# open PR: codex/update-<ver>-align -> codex/update-<ver>-ingest
```

Then land the ingest branch directly on `main` with ancestry preserved:

```sh
git switch main
git merge --ff-only codex/update-<ver>-ingest
git push origin main
```

### 8. Open or retarget the alignment PR to `main`

Before opening or retargeting the alignment PR to `main`, run:

```sh
git merge-base --is-ancestor codex/update-<ver>-ingest main
git log --oneline main..codex/update-<ver>-align
```

The first command must pass. The second command must show only fork-owned
alignment commits.

If using Path B, create the review branch from the updated `main`:

```sh
git switch main
git pull --ff-only origin main
git switch -c codex/update-<ver>-align
```

- Put only fork-specific alignment commits here.
- Open the PR from this branch into fork `main`.
- This is the only branch that should receive bot review.

## Anti-pattern

Do not do this:

1. branch from upstream `rust-v*`
2. replay fork work there
3. open a PR into fork `main`

That creates synthetic GitHub conflicts because the branch is missing the
already-merged fork history.
