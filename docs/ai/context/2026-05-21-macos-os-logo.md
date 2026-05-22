# 2026-05-21 macOS OS logo fix

## Context

When running the agent locally on macOS, the Komari UI showed a Windows system logo.

## Finding

The agent does not send a dedicated logo field. `BasicInfo` sends `os`, `version`, `kernel_version`, and related host data. The OS logo is therefore inferred downstream from the `os` string.

On macOS, using the raw sysinfo platform name can expose Darwin-oriented naming instead of the product name expected by dashboard logo mapping.

## Change

- `src/get_info/os.rs` now normalizes the platform name to `macOS` on `target_os = "macos"`.
- OS display-name composition is isolated in `compose_os_display_name` and covered with unit tests for version formatting and empty-name fallback.

## Maintenance note

If future dashboard logo mismatches appear, inspect the serialized `BasicInfo.os` value first. Avoid adding a client-side logo field unless the server protocol explicitly supports it.
