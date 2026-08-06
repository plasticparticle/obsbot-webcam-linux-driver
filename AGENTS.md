# Repository Guidelines

## Project Structure & Module Organization

This repository is a Rust 2021 workspace. `crates/obsbot-core/` contains device discovery, V4L2 controls, and the UVC extension-unit protocol. `crates/obsbot-cli/` builds the `obsbotctl` command-line client, while `crates/obsbot-gui/` builds the native egui control center and its preview capture thread. Protocol notes live in `docs/protocol.md`; raw device observations belong in `docs/device-recon/`. Treat `target/` as generated output and do not commit it.

## Build, Test, and Development Commands

- `cargo build --workspace` builds every crate in debug mode.
- `cargo build --release` produces `target/release/obsbotctl` and `target/release/obsbot-control-center`.
- `cargo run -p obsbot-cli -- status` runs the CLI against a connected camera.
- `cargo run -p obsbot-gui` launches the native GUI.
- `cargo test --workspace` runs all unit and documentation tests.
- `cargo fmt --all -- --check` verifies formatting; `cargo clippy --workspace -- -D warnings` rejects lint warnings.

Hardware commands require an accessible OBSBOT video node under `/dev/video*`. Keep tests deterministic and hardware-independent where possible.

## Coding Style & Naming Conventions

Use standard `rustfmt` output and four-space indentation. Follow Rust naming: `snake_case` for functions, modules, and variables; `CamelCase` for structs and enums; `SCREAMING_SNAKE_CASE` for constants. Keep camera protocol encoding in `obsbot-core`; frontends should call its typed API rather than duplicate byte-level commands. Return contextual errors instead of panicking during normal device discovery or I/O.

## Testing Guidelines

Place focused unit tests beside the implementation in `#[cfg(test)] mod tests`; existing protocol-frame tests are in `crates/obsbot-core/src/xu.rs`. Name tests for observable behavior, such as `recall_frame_matches_capture`. Add regression tests for protocol changes and run the full workspace suite before submitting.

## Commit & Pull Request Guidelines

Git history is not present in this checkout, so no existing message convention can be verified. Use short, imperative subjects such as `Polish GUI control cards`, and keep unrelated changes separate. Pull requests should explain behavior and hardware tested, link relevant issues, list validation commands, and include screenshots for GUI changes. Call out protocol discoveries and update `docs/protocol.md` with supporting captures when applicable.
