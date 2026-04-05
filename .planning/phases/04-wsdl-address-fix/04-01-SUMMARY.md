---
phase: 04-wsdl-address-fix
plan: "01"
subsystem: server, wsdl
tags: [bug-fix, wsdl, soap-address, multi-service, integration-test]
dependency_graph:
  requires:
    - "03-01"  # multi-service WSDL GET handler existed
  provides:
    - correct per-service soap:address URL in multi-service WSDL GET responses
  affects:
    - src/server.rs
    - tests/integration_test.rs
    - .planning/phases/03-audit-gap-closure/03-02-SUMMARY.md
tech_stack:
  added: []
  patterns:
    - axum MatchedPath extractor for per-route path introspection
    - Option<MatchedPath> fallback pattern for safe handler extraction
key_files:
  created: []
  modified:
    - src/server.rs
    - tests/integration_test.rs
    - .planning/phases/03-audit-gap-closure/03-02-SUMMARY.md
decisions:
  - "Option<MatchedPath> used instead of MatchedPath to avoid axum 500 on missing extractor — in practice always present via router.route()"
  - "03-02-SUMMARY.md requirements-completed corrected from [WSDL-04, DSP-06] to [ENV-05, ENV-06, FLT-04, FLT-05]"
requirements-completed: [WSDL-04, HTTP-03]
metrics:
  duration: "~8 minutes"
  completed: "2026-04-05"
  tasks_completed: 2
  files_modified: 3
---

# Phase 4 Plan 01: WSDL Address Fix Summary

One-liner: Fixed wsdl_get_handler to use axum MatchedPath extractor so multi-service WSDL GET returns correct per-service soap:address URL, not the mount_path.

## What Was Built

Two targeted fixes plus a metadata correction:

1. `src/server.rs` — `wsdl_get_handler` now accepts `matched_path: Option<MatchedPath>` as first parameter. The server URL is derived from the matched route path (e.g., `/soap/a`) rather than `svc.mount_path` (e.g., `/soap`). A SOAP client reading the WSDL now gets the correct endpoint URL to POST to.

2. `tests/integration_test.rs` — New test `multi_service_wsdl_get_returns_correct_address` builds a multi-service SoapService (ServiceA at /soap/a, ServiceB at /soap/b), calls GET /soap/a?wsdl and GET /soap/b?wsdl, and asserts each response body contains the per-service path.

3. `.planning/phases/03-audit-gap-closure/03-02-SUMMARY.md` — Fixed copy-paste error in frontmatter: `requirements-completed` corrected from `[WSDL-04, DSP-06]` to `[ENV-05, ENV-06, FLT-04, FLT-05]` (plan 03-02 implemented ENV/FLT documentation fixes, not WSDL/DSP items).

## Tasks Completed

| Task | Name | Commit | Files |
|------|------|--------|-------|
| 1 | Fix wsdl_get_handler to use MatchedPath for per-service URL | 783e7af | src/server.rs |
| 2 | Add multi-service WSDL GET integration test + fix 03-02-SUMMARY.md | 0a3d026 | tests/integration_test.rs, .planning/phases/03-audit-gap-closure/03-02-SUMMARY.md |

## Verification

- `cargo check` passes cleanly after Task 1
- `cargo test` passes with 205 tests (186 unit + 12 integration + 7 ONVIF) after Task 2
- New test `multi_service_wsdl_get_returns_correct_address` is in the 12 integration test count
- GET /soap/a?wsdl body contains `/soap/a` — confirmed by passing test
- GET /soap/b?wsdl body contains `/soap/b` — confirmed by passing test
- 03-02-SUMMARY.md line 27: `requirements-completed: [ENV-05, ENV-06, FLT-04, FLT-05]` — confirmed

## Decisions Made

1. `Option<MatchedPath>` used rather than `MatchedPath` to prevent axum returning HTTP 500 on extractor failure. In practice, MatchedPath is always populated when a handler is registered via `router.route()`, but Option is the safe/idiomatic pattern.
2. The `unwrap_or(&svc.mount_path)` fallback means single-service mode behavior is preserved — if MatchedPath is somehow absent, the old behavior applies.

## Deviations from Plan

None — plan executed exactly as written.

## Self-Check: PASSED
