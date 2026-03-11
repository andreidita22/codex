# Fork update workflow

This fork is easiest to maintain when the customization stays as a very small
linear patch on top of an official Codex release tag.

Recommended workflow:

1. Keep `main` close to upstream release tags.
2. Keep your custom behavior on a dedicated branch such as `codex/continuation-bridge`.
3. Keep the bolt-on patch stack short and focused.
4. Rebase that branch onto each new `rust-v*` release tag instead of merging upstream history.

One-time setup:

```sh
git config --local rerere.enabled true
git config --local rerere.autoupdate true
```

That lets Git reuse the same conflict resolutions on future release bumps.

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
- refreshes workspace package version entries in [Cargo.lock](/home/rose/work/codex/fork/codex-rs/Cargo.lock) without regenerating the dependency graph

Assumption:

- the current branch is a small linear customization branch built directly on top
  of a previous `rust-v*` release tag

Fast path after the rebase:

1. Confirm the branch still only contains the bolt-on commits on top of the new
   tag:

   ```sh
   git log --oneline <new-rust-tag>..HEAD
   ```

2. If conflicts occur, expect the hottest files to be:
   - `codex-rs/core/src/config/mod.rs`
   - `codex-rs/core/config.schema.json`

3. Regenerate the config schema and run the targeted validation for the bolt-on:

   ```sh
   cd codex-rs
   env PKG_CONFIG_PATH=/tmp/codex-libcap/extracted/usr/lib/x86_64-linux-gnu/pkgconfig \
       PKG_CONFIG_SYSROOT_DIR=/tmp/codex-libcap/extracted \
       just write-config-schema
   env PKG_CONFIG_PATH=/tmp/codex-libcap/extracted/usr/lib/x86_64-linux-gnu/pkgconfig \
       PKG_CONFIG_SYSROOT_DIR=/tmp/codex-libcap/extracted \
       cargo test -p codex-core continuation_bridge
   env PKG_CONFIG_PATH=/tmp/codex-libcap/extracted/usr/lib/x86_64-linux-gnu/pkgconfig \
       PKG_CONFIG_SYSROOT_DIR=/tmp/codex-libcap/extracted \
       cargo test -p codex-core --test all compact
   just fix -p codex-core
   just fmt
   ```

   Note:
   the rebase helper already refreshes the workspace-version entries in
   [Cargo.lock](/home/rose/work/codex/fork/codex-rs/Cargo.lock), so you should
   not need a separate lockfile-only commit after a routine release bump.

Optional slow path:

- Only build the distributable binary if you actually need it for packaging or
  an external client such as the VS Code extension:

  ```sh
  cd codex-rs
  env CARGO_NET_GIT_FETCH_WITH_CLI=true \
      PKG_CONFIG_PATH=/tmp/codex-libcap/extracted/usr/lib/x86_64-linux-gnu/pkgconfig \
      PKG_CONFIG_SYSROOT_DIR=/tmp/codex-libcap/extracted \
      cargo build -p codex-cli --release
  ```

The release binary is intentionally slow to build because the workspace uses the
full release profile with fat LTO. Treat that as packaging, not as part of the
default update loop.
