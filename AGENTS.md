# Repository Guidelines

## Project Structure & Module Organization
- `crates/` is the Rust workspace (e.g., core app in `crates/zed`, UI framework in `crates/gpui`, extension plumbing in `crates/extension_*`).
- `extensions/` contains example and test extensions.
- `docs/` holds mdBook docs (`docs/src` for content; `docs/AGENTS.md` for doc automation rules).
- `assets/`, `ci/`, `tooling/`, and `script/` provide shared assets, CI config, tooling, and developer scripts.

## Build, Test, and Development Commands
- `cargo run -q` / `cargo run -q --release` build and launch Zed in debug or release mode.
- `cargo test --workspace -q` runs the Rust test suite across all crates.
- `./script/clippy` runs linting (preferred over `cargo clippy`).
- Optional: `cargo nextest run --workspace --no-fail-fast` for faster test runs (if `cargo-nextest` is installed).
- Docs preview: `mdbook serve docs` (see `docs/README.md` for prerequisites).

## Coding Style & Naming Conventions
- Rustfmt and Clippy are part of the toolchain (`rust-toolchain.toml`).
- Avoid `unwrap()`; propagate errors with `?` or handle them explicitly. Do not silently discard errors with `let _ =`.
- Prefer full-word identifiers; avoid abbreviations.
- Avoid `mod.rs`; use `src/<module>.rs`. For new crates, set `[lib] path = "<crate_name>.rs"` in `Cargo.toml`.
- Comments should explain *why*, not restate what the code already says.

## Testing Guidelines
- Tests often live beside source (`crates/**/src/test*.rs`) and data fixtures (e.g., `crates/vim/test_data/`).
- Run `cargo test --workspace -q`. For UI changes, consult platform docs for visual regression tests (e.g., `docs/src/development/macos.md`).

## Commit & Pull Request Guidelines
- Commit messages are short, sentence-case summaries; area prefixes like `vim:` or `language_models:` are common and PR numbers appear as `(#12345)`.
- PRs should explain what and why, include tests, attach screenshots for UI changes, and keep the change focused. Feature additions should be confirmed via issue or discussion first (see `CONTRIBUTING.md`).

## Agent-Specific Notes
- If using automation, follow Rust guidance in `CLAUDE.md`/`GEMINI.md` and documentation rules in `docs/AGENTS.md` when editing docs.
