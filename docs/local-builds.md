# Local Build Policy

This fork is for local use. The main risk is not source size, it is Cargo build
artifact churn (especially full-workspace `cargo test`).

Goals:

- keep one stable runnable Codex binary for VS Code
- keep fast local iteration available
- isolate high-churn Cargo outputs away from repo `target/` when possible
- make cleanup deterministic

## Runtime Binary Path

Use a stable binary path for IDE integration:

`/home/rose/work/codex/fork/bin/codex-vscode`

That path currently points to:

`/home/rose/work/codex/fork/codex-rs/target/release/codex`

## Cache Root Convention

Preferred cache root:

- `/mnt/rust-build` (dedicated ext4 VHD mounted in WSL)

`cargo-workflow.sh` now treats this as required by default.
If `/mnt/rust-build` is not mounted, it auto-attaches it by default.

You can still override the root explicitly for all helper commands:

```bash
export CODEX_RUST_CACHE_ROOT=/path/to/cache-root
```

Auto-attach is on by default. To disable it for a run:

```bash
CODEX_AUTO_ATTACH_RUST_DISK=0 /home/rose/work/codex/fork/scripts/cargo-workflow.sh full-test
```

## Cargo Modes

Use the helper:

```bash
/home/rose/work/codex/fork/scripts/cargo-workflow.sh
```

Plain `cargo` is also intercepted now for repos that expose
`scripts/cargo-workflow.sh`.
The shell shim ensures `/mnt/rust-build` is mounted before delegating to the
real Cargo, and the shim is responsible for exporting the dedicated target path
for local fork sessions.

### `fast` mode

Use for compile-validation and short loops.

- target dir: `<cache-root>/cargo-target/codex-rs/fast`
- keeps incremental enabled
- uses lighter dev debug settings
- default command: `cargo check -p codex-cli`

Examples:

```bash
/home/rose/work/codex/fork/scripts/cargo-workflow.sh fast
/home/rose/work/codex/fork/scripts/cargo-workflow.sh fast test -p codex-core compact
```

### `full-test` mode

Use for full workspace runs.

- target dir: `<cache-root>/cargo-target/codex-rs/full-test`
- disables incremental
- disables debug info for dev/test to reduce disk growth
- default command: `cargo test`

Examples:

```bash
/home/rose/work/codex/fork/scripts/cargo-workflow.sh full-test
/home/rose/work/codex/fork/scripts/cargo-workflow.sh full-test test -p codex-app-server --test all
```

## Size and Prune Operations

Show isolated cache sizes:

```bash
/home/rose/work/codex/fork/scripts/cargo-workflow.sh sizes
```

Prune one lane:

```bash
/home/rose/work/codex/fork/scripts/cargo-workflow.sh prune-fast
/home/rose/work/codex/fork/scripts/cargo-workflow.sh prune-full-test
```

Prune all isolated codex-rs caches:

```bash
/home/rose/work/codex/fork/scripts/cargo-workflow.sh prune-all
```

## Repo-Local `target/` Prune

Use existing helper when repo-local `codex-rs/target` has accumulated artifacts:

```bash
/home/rose/work/codex/fork/scripts/prune-build-artifacts.sh sizes
/home/rose/work/codex/fork/scripts/prune-build-artifacts.sh medium
```

## Recommended Routine

### Operator Session Start (every new fork work session)

1. Check cache root/mount:

```bash
/home/rose/work/codex/fork/scripts/cargo-workflow.sh root
```

2. The helper will reattach the VHD automatically when needed.
Manual Windows fallback remains:

```powershell
wsl --mount --vhd C:\WSL\rust-build.vhdx --bare
```

Alternative helper (auto-elevates):

```powershell
powershell -ExecutionPolicy Bypass -File "$env:USERPROFILE\Desktop\reattach-rust-disk.ps1"
```

3. Run start-of-session prune (clears old full-test cache lane):

```bash
/home/rose/work/codex/fork/scripts/cargo-workflow.sh session-start
```

4. Use `fast` for iteration and `full-test` only when needed:

```bash
/home/rose/work/codex/fork/scripts/cargo-workflow.sh fast
/home/rose/work/codex/fork/scripts/cargo-workflow.sh full-test
```

5. Keep VS Code pointed at `bin/codex-vscode`.
