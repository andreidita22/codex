# Local Build Policy

This fork is for local use in VS Code. The goal is:

- keep one stable runnable Codex binary
- keep enough Cargo build state for warm rebuilds
- avoid piling up `target/` variants that bloat WSL storage

## Current Disk Picture

A fresh `cargo build -p codex-cli` in this repo produced roughly:

- `target/debug/deps`: `7.4G`
- `target/debug/incremental`: `4.4G`
- `target/debug/build`: `472M`
- `target/debug/codex`: `1.5G`

By comparison, Cargo download caches were much smaller:

- `~/.cargo`: about `1.1G`

So the main storage problem is `codex-rs/target/`, not Cargo registry downloads.

## Stable VS Code Path

Do not point VS Code at `codex-rs/target/debug/codex` directly. That path disappears after cleanup.

Instead, build and copy the current debug binary to a stable path outside `target/`:

```bash
/home/rose/work/codex/fork/scripts/build-vscode-binary.sh
```

That creates:

`/home/rose/work/codex/fork/bin/codex-vscode`

Use that path for `chatgpt.cliExecutable`.

## Build Policy

For everyday local use:

1. Build debug only.
2. Do not build `release` unless you explicitly need packaging or perf testing.
3. Refresh the stable VS Code binary with:

```bash
/home/rose/work/codex/fork/scripts/build-vscode-binary.sh
```

The script:

- builds `codex-cli` in debug mode
- copies the current binary to `bin/codex-vscode`
- strips debug symbols from the copied runtime binary when `strip` is available

The copy keeps VS Code working even if `target/` is cleaned later.

## Cleanup Policy

Use the repo helper:

```bash
/home/rose/work/codex/fork/scripts/prune-build-artifacts.sh sizes
```

Modes:

- `light`
  - remove `target/release`
  - remove `target/rust-analyzer`
  - remove stale `target/debug/incremental/*` older than 7 days
- `medium`
  - do `light`
  - remove all `target/debug/incremental`
- `deep`
  - do `medium`
  - run `cargo clean`

## Recommended Routine

Normal iteration:

1. Build with `scripts/build-vscode-binary.sh`
2. Keep `target/debug/deps` and `target/debug/build`
3. Run `scripts/prune-build-artifacts.sh light` after release builds or after big update/rebase work

When `target/` starts getting too large:

- If `target/` is above roughly `15G`, run `medium`
- If you want to reclaim most space and accept a colder next rebuild, run `deep`

## What To Keep

Keep:

- `bin/codex-vscode`
- `~/.cargo/registry`
- `~/.cargo/git`
- `~/.rustup/toolchains`

These are reusable and much smaller than repeated `target/` growth.

Do not keep:

- old `target/release`
- old `target/rust-analyzer`
- stale `target/debug/incremental`

Those are the least valuable bytes.
