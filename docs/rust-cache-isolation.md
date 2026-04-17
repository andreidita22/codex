# Rust Cache Isolation (WSL)

This note defines a low-risk setup for controlling Cargo disk growth during
local `codex-rs` development.

## 1. Mount Convention

Use a dedicated ext4 mount inside WSL:

- preferred mount point: `/mnt/rust-build`

The helper now enforces this mount by default and auto-attaches it when missing.
It no longer silently falls back to other roots.

You can override the root explicitly:

```bash
export CODEX_RUST_CACHE_ROOT=/your/cache/root
```

Auto-attach is on by default. To disable it for a run:

```bash
CODEX_AUTO_ATTACH_RUST_DISK=0 /home/rose/work/codex/fork/scripts/cargo-workflow.sh full-test
```

Reattach command (Windows):

```powershell
wsl --mount --vhd C:\WSL\rust-build.vhdx --bare
```

Helper scripts:

- Windows desktop helper: `C:\Users\Rose\Desktop\reattach-rust-disk.ps1`
- Repo helper (supports `-NoPause` for automation): `/home/rose/work/codex/fork/scripts/reattach-rust-disk.ps1`

One-time VHD setup helpers:

- Windows (Admin): `/home/rose/work/codex/fork/scripts/setup-rust-vhd.ps1`
- WSL (root): `/home/rose/work/codex/fork/scripts/finalize-rust-vhd-mount.sh /dev/sdX`

`finalize-rust-vhd-mount.sh` also prepares `/mnt/rust-build/cargo-target` for a
normal user session. If user inference is ambiguous, re-run it with
`CODEX_RUST_CACHE_OWNER=<user>`.

For plain `cargo` use, shell startup prepends a `cargo` shim directory.
The shim walks upward from the current directory, and if it finds a repo root
with `scripts/cargo-workflow.sh`, it runs `ensure-mount` before delegating to
the real Cargo proxy from rustup.

## 2. Cargo Config (optional global baseline)

The workflow script already exports the relevant env vars, so this step is
optional. If you want a baseline in `~/.cargo/config.toml`, use:

```toml
[target.x86_64-unknown-linux-gnu]
linker = "clang"
rustflags = ["-C","link-arg=-fuse-ld=lld"]
```

Do not hardcode `build.target-dir` globally unless you want every Rust project
to share one root; use per-workspace command wrappers instead.

## 3. Two Command Modes

Use:

`/home/rose/work/codex/fork/scripts/cargo-workflow.sh`

Start-of-session hygiene:

```bash
/home/rose/work/codex/fork/scripts/cargo-workflow.sh root
/home/rose/work/codex/fork/scripts/cargo-workflow.sh session-start
```

### Fast local mode

```bash
/home/rose/work/codex/fork/scripts/cargo-workflow.sh fast
```

Default:

- command: `cargo check -p codex-cli`
- target root: `<cache-root>/cargo-target/codex-rs/fast`
- profile behavior: lighter dev debug, incremental on

### Full-test mode

```bash
/home/rose/work/codex/fork/scripts/cargo-workflow.sh full-test
```

Default:

- command: `cargo test`
- target root: `<cache-root>/cargo-target/codex-rs/full-test`
- profile behavior: debug minimized, incremental off

## 4. Size and Cleanup

```bash
/home/rose/work/codex/fork/scripts/cargo-workflow.sh sizes
/home/rose/work/codex/fork/scripts/cargo-workflow.sh prune-fast
/home/rose/work/codex/fork/scripts/cargo-workflow.sh prune-full-test
/home/rose/work/codex/fork/scripts/cargo-workflow.sh prune-all
```

For repo-local cleanup of legacy `codex-rs/target`:

```bash
/home/rose/work/codex/fork/scripts/prune-build-artifacts.sh medium
```

## 5. Why This Shape

- isolates high-churn test/build artifacts from repo source
- prevents mixed fast-loop and full-test caches from stacking blindly
- keeps cleanup bounded and lane-specific
- avoids global Cargo behavior changes for unrelated projects
