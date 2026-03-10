# Fork update workflow

This fork is easiest to maintain when the customization stays as a very small
linear patch on top of an official Codex release tag.

Recommended workflow:

1. Keep `main` close to upstream release tags.
2. Keep your custom behavior on a dedicated branch such as `codex/continuation-bridge`.
3. Keep the bolt-on patch stack short and focused.
4. Rebase that branch onto each new `rust-v*` release tag instead of merging upstream history.

To automate the rebase step, this repo includes:

```sh
scripts/rebase-bolt-on-release.sh latest
```

You can also pin a specific release:

```sh
scripts/rebase-bolt-on-release.sh rust-v0.111.0
```

What the script does:

- requires a clean worktree
- fetches tags from the `upstream` remote
- infers the current branch's merged `rust-v*` base tag
- creates a timestamped `backup/...` branch
- rebases the current branch from the old release tag onto the new one

Assumption:

- the current branch is a small linear customization branch built directly on top
  of a previous `rust-v*` release tag

After the rebase, run the same targeted checks you use for the custom patch, and
only then decide whether to push the rebased branch.
