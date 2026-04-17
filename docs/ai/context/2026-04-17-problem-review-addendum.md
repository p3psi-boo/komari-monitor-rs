# PROBLEM.md additional review entries

Date: 2026-04-17

## Summary

Appended extra review findings to `PROBLEM.md` and kept them grouped under the existing priority sections.

## Added items

### High priority

- netlink connection counting includes `NLMSG_DONE` / `NLMSG_ERROR`
- netlink header parsing uses unaligned pointer dereference
- callback handling has no concurrency limit
- IPv6 TCP ping address formatting is invalid
- dual HTTP backends can both execute and duplicate/override results

### Medium priority

- remote exec lacks timeout control
- token and request_id are concatenated into query strings without encoding
- callback websocket read errors are silently ignored
- `--tls` and URL scheme both control transport semantics
- interval parameters lack lower-bound validation
- process counting is Linux-specific and silently wrong on other platforms
- websocket connection errors are over-normalized

### Low priority

- CPU name output order is unstable due to `HashSet` iteration order

## Rationale

The user asked to append non-`PROBLEM.md` review findings into `PROBLEM.md` and distinguish them by priority. No code changes were made.