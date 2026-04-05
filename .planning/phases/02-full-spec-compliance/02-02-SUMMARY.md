---
phase: 02-full-spec-compliance
plan: "02"
requirements-completed: [FLT-04, FLT-05]
subsystem: fault
tags: [soap11, fault-serialization, versioned-dispatch, integration-tests]
dependency_graph:
  requires:
    - "02-01"  # SoapVersion type and fault_response signature
  provides:
    - to_xml_bytes_v11 on SoapFault
    - to_xml_bytes_versioned dispatch method
    - SOAP 1.1 fault serialization
  affects:
    - src/fault.rs
    - src/server.rs
    - tests/integration_test.rs
tech_stack:
  added: []
  patterns:
    - versioned dispatch via match on SoapVersion enum
    - TDD red/green for fault unit tests and integration tests
key_files:
  created: []
  modified:
    - src/fault.rs
    - src/server.rs
    - tests/integration_test.rs
decisions:
  - "to_xml_bytes_v11() is private (fn not pub fn) — external callers use to_xml_bytes_versioned(&SoapVersion)"
  - "DataEncodingUnknown maps to SOAP-ENV:Server in SOAP 1.1 (no equivalent), matching Apache CXF behavior"
  - "Detail element in SOAP 1.1 is unqualified <detail>, not <env:Detail>"
  - "resp.as_bytes() not resp.bytes() — axum-test TestResponse API uses as_bytes()"
metrics:
  duration: "~3 minutes"
  completed: "2026-04-05"
  tasks_completed: 2
  files_modified: 3
---

# Phase 2 Plan 02: SOAP 1.1 Fault Serializer Summary

One-liner: SOAP 1.1 fault serialization with flat faultcode/faultstring structure, version-aware code mapping (Sender->Client, Receiver->Server), and full integration test coverage.

## What Was Built

Added SOAP 1.1 fault serialization to the existing `SoapFault` struct. The existing `to_xml_bytes()` (SOAP 1.2, nested Code/Reason) was left unchanged. Two new methods were added:

- `to_xml_bytes_v11()` — private method producing flat SOAP 1.1 faultcode/faultstring structure with correct `SOAP-ENV:` namespace prefix
- `to_xml_bytes_versioned(&SoapVersion)` — public dispatch method: routes to `to_xml_bytes()` for SOAP 1.2, `to_xml_bytes_v11()` for SOAP 1.1

The `fault_response()` function in server.rs was updated to call `to_xml_bytes_versioned(&version)` instead of the unconditional `to_xml_bytes()`, so SOAP 1.1 requests now receive correctly structured fault envelopes.

Three integration tests were added verifying end-to-end SOAP 1.1 behavior.

## Tasks Completed

| Task | Name | Commit | Files |
|------|------|--------|-------|
| 1 | Add SOAP 1.1 fault serializer to fault.rs | 347a367 | src/fault.rs |
| 2 | Wire versioned fault serializer + integration tests | 8b993f7 | src/server.rs, tests/integration_test.rs |

## Verification

- `cargo test fault::` — 26 tests pass (12 new SOAP 1.1 tests + 14 existing SOAP 1.2 tests)
- `cargo test soap11` — soap11_end_to_end, soap11_fault_has_correct_structure, soap11_fault_content_type_is_text_xml all pass
- Full `cargo test` — 204 tests pass, no regressions

## Decisions Made

1. `to_xml_bytes_v11()` is `fn` (private), not `pub fn` — external callers should use the versioned dispatcher to avoid bypassing version routing
2. `DataEncodingUnknown` maps to `SOAP-ENV:Server` in SOAP 1.1, matching Apache CXF behavior (no SOAP 1.1 equivalent for this code)
3. `<detail>` element in SOAP 1.1 uses unqualified name (no namespace prefix), per W3C SOAP 1.1 spec Section 4.4
4. axum-test's `TestResponse::as_bytes()` is the correct method (not `.bytes()`) — this was caught during GREEN phase compilation

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] axum-test API: `.bytes()` does not exist on TestResponse**
- **Found during:** Task 2 (GREEN compilation)
- **Issue:** The plan suggested using `.bytes()` on `TestResponse`. The actual method is `.as_bytes()`.
- **Fix:** Changed all `resp.bytes()` calls to `resp.as_bytes()` in the three new integration tests
- **Files modified:** tests/integration_test.rs
- **Commit:** 8b993f7 (included in Task 2 commit)

## Self-Check: PASSED
