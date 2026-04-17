# Repository Notes

## Verification

- Preferred verification order: `cargo check` -> `cargo clippy --all-targets --all-features -- -D warnings` -> `cargo test --all-features` -> `cargo build --all-features`
- On NixOS, prefer running commands through the project dev shell when toolchain components are missing from the host environment.

## Documentation

- Mirror code-review findings and change rationale into `docs/ai/context/` for cross-session continuity.

## 2026-04-17

- Appended additional review findings into `PROBLEM.md`, keeping entries grouped by priority.
- Extra issues added beyond the original file included netlink counting correctness/UB, callback concurrency limits, IPv6 TCP ping formatting, duplicate dual-backend HTTP behavior, exec timeout, URL query encoding, callback read error handling, transport-layer config ambiguity, missing interval validation, Linux-only process counting, over-normalized websocket errors, and unstable CPU-name ordering.
- Implemented most `PROBLEM.md` fixes and removed `--tls` from runtime/Nix/README in favor of URL scheme as transport source of truth (`ws`/`wss`, `http`/`https`).
- Verification state after implementation: all stages pass — `cargo check --all-features`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-features`, `cargo build --all-features`. All 29 PROBLEM.md items addressed via parallel subagent batch implementation.
