---
phase: 02-full-spec-compliance
plan: "01"
subsystem: envelope
tags: [rust, soap, soap11, envelope, content-type, fault]

# Dependency graph
requires:
  - phase: 01-onvif-level-support
    provides: "parse_envelope(), serialize_envelope(), fault_response(), SoapVersion enum"
provides:
  - "SOAP 1.1 envelope parse/serialize unit test coverage"
  - "fault_response(SoapFault, SoapVersion) version-aware Content-Type"
affects: [future phases needing SOAP 1.1 fault handling]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "fault_response() now accepts SoapVersion parameter for version-aware Content-Type selection"
    - "Pre-detection fault responses default to SoapVersion::Soap12 (no version context available)"

key-files:
  created: []
  modified:
    - src/envelope.rs
    - src/server.rs

key-decisions:
  - "fault_response signature changed to fault_response(SoapFault, SoapVersion)"
  - "Pre-detection faults (step 1) default to Soap12 — acceptable per spec since no version context exists"
  - "Post-detection faults pass envelope.soap_version.clone() for correct Content-Type"
  - "response_content_type(&version) replaces hard-coded Content-Type string"

patterns-established:
  - "Version-aware fault responses via response_content_type() lookup"

requirements-completed: [ENV-05, ENV-06]

# Metrics
duration: 6min
completed: 2026-04-05
---

# Phase 2 Plan 01: SOAP 1.1 Envelope Tests & Content-Type Fix Summary

**SOAP 1.1 envelope unit test coverage and version-aware fault_response() Content-Type fix.**

## Performance

- **Duration:** ~6 min
- **Started:** 2026-04-05T05:40:00Z
- **Completed:** 2026-04-05T05:46:10Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments

- Added 6 SOAP 1.1 envelope unit tests: parse, parse with header, body first child, serialize, response_content_type for both 1.1 and 1.2
- Changed `fault_response(SoapFault)` to `fault_response(SoapFault, SoapVersion)` using `response_content_type()` for correct Content-Type
- Updated all 7 call sites in `soap_post_handler` to pass version parameter

## Task Commits

Each task was committed atomically:

1. **Task 1: Add SOAP 1.1 envelope unit tests** - `6990958` (feat)
2. **Task 2: Fix fault_response() to accept and use SoapVersion** - `3964dbe` (fix)

## Files Created/Modified

- `/Users/jhogendorn/ws/soap-server/src/envelope.rs` - Added 6 SOAP 1.1 unit tests (parse, serialize, content-type)
- `/Users/jhogendorn/ws/soap-server/src/server.rs` - Changed fault_response signature, updated all call sites, added 2 unit tests

## Decisions Made

- Pre-detection faults default to Soap12 — no version context available at step 1
- SoapVersion::clone() used for post-detection call sites; moved into serialize_envelope at final use

## Deviations from Plan

None.

## Issues Encountered

None.

## Next Phase Readiness

- ENV-05 (SOAP 1.1 envelope support) and ENV-06 (version-aware Content-Type) requirements satisfied
- All 189 tests pass (174 unit + 8 integration + 7 ONVIF)

---
*Phase: 02-full-spec-compliance*
*Completed: 2026-04-05*

## Self-Check: PASSED

- FOUND: .planning/phases/02-full-spec-compliance/02-01-SUMMARY.md
- FOUND: src/envelope.rs
- FOUND: src/server.rs
- FOUND commit 6990958: feat(02-01): add SOAP 1.1 envelope unit tests
- FOUND commit 3964dbe: fix(02-01): fault_response() now accepts SoapVersion for correct Content-Type
