# Selflow

- Read the spec (`docs/superpowers/specs/`) and the roadmap before planning.
- English everywhere: code, identifiers, logs, docs.
- No comments in the code.
- Layout: `main.rs` holds only `main`; `lib.rs` only declares modules; one module per subject, which becomes a folder with `mod.rs`, `types.rs` and `constants.rs` as it grows; each subject module owns its `thiserror` error enum; CLI commands live in `src/cli/commands/<name>_cmd.rs` with `<Name>Args` and `<name>_cmd()`; operating system code lives in `selflow-platform` (shared) or the binary's `src/os/` (its own), one file per system (`unix.rs`) picked with `#[cfg]`; `selflow-core`, `selflow-i18n` and `selflow-protocol` hold none, and `make check` builds them for Windows.
- Conventional Commits, enforced by the `commit-msg` hook (`git config core.hooksPath .githooks`).
- Check every dependency on crates.io (not deprecated, archived or yanked) before adding it.
- Run `make check` before calling work done.
- When the shell isn't proto-activated, run cargo through `proto exec rust -- cargo ...`; the stable rustup toolchain is older than the pinned one.
