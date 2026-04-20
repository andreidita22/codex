# Fork Update Checklist

Use this as the fast refresh for release ingests. For rationale and edge cases,
see [fork-updates.md](fork-updates.md).

## Canonical rule

- Treat fork `main` as canonical.
- Treat `upstream-main` as a read-only mirror of the latest upstream
  `rust-v*` release.
- Start every release ingest branch from current fork `main`, never from
  `upstream-main`.
- Do not open a PR for the raw upstream-ingest branch.
- Open PRs only for the later fork-alignment branch that contains our commits.

## Standard sequence

### 1. Move the upstream mirror

```sh
git fetch upstream --tags --prune
git branch -f upstream-main rust-v0.120.0
git push origin upstream-main:upstream-main
```

### 2. Start from fork `main`

```sh
git switch main
git pull --ff-only origin main
git switch -c codex/update-0.120-ingest
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
git diff --stat main..upstream-main
git diff --name-only main...upstream-main
```

- Use [custom-fork-module-inventory.md](custom-fork-module-inventory.md) to
  prioritize seam files.

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

### 7. Land the ingest branch directly on `main`

```sh
git switch main
git merge --ff-only codex/update-0.120-ingest
git push origin main
```

### 8. Create the review branch for our commits only

```sh
git switch main
git pull --ff-only origin main
git switch -c codex/update-0.120-align
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
