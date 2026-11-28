# Selflow

- Read the spec (`docs/superpowers/specs/`) and the roadmap before planning.
- English everywhere: code, identifiers, logs, docs.
- No comments in the code.
- Layout: `main.rs` holds only `main`; `lib.rs` only declares modules; one module per subject, which becomes a folder with `mod.rs`, `types.rs` and `constants.rs` as it grows; each subject module owns its `thiserror` error enum; CLI commands live in `src/cli/commands/<name>_cmd.rs` with `<Name>Args` and `<name>_cmd()`; operating system code lives in `selflow-platform` (shared) or the binary's `src/os/` (its own), one file per system (`unix.rs`) picked with `#[cfg]`; `selflow-core`, `selflow-i18n` and `selflow-protocol` hold none, and `make check` builds them for Windows.
- Conventional Commits, enforced by the `commit-msg` hook (`git config core.hooksPath .githooks`).
- Check every dependency on crates.io (not deprecated, archived or yanked) before adding it.
- Run `make check` before calling work done.
- When the shell isn't proto-activated, run cargo through `proto exec rust -- cargo ...`; the stable rustup toolchain is older than the pinned one.

<!-- prerelease:start -->
<!-- prerelease:remove-when: first version tag or first long-lived development branch besides main -->
## No release or users yet

Selflow has no release and no users besides the owner, who runs it on their own machine. Large rewrites and deletions are fine: delete or reshape crates, modules, CLI commands, config fields, IPC messages and the Claude Code hooks without a compatibility layer, and change the SQLite schema by editing the migration in `crates/selflowd/src/store/migrations.rs` in place instead of appending a new one; delete `$XDG_STATE_HOME/selflow/selflow.db` to start clean afterwards. The IPC version handshake still applies, since a running daemon and fresh clients can disagree after any build. Don't keep transition code, and don't drop a change because it breaks the owner's local state. Pick the best solution, not the one that breaks the least.

**Remove this section with the first version tag or the first long-lived development branch besides `main`.** From then on, applied migrations no longer change and every schema change goes into a new migration.
<!-- prerelease:end -->
