---
phase: 01-onvif-level-support
plan: "04"
subsystem: envelope-wssec
tags: [rust, soap, envelope, wssec, tdd, quick-xml, chrono]

requires:
  - 01-02

provides:
  - src/envelope.rs with parse_envelope(), serialize_envelope(), detect_soap_version()
  - src/wssec/timestamp.rs with parse_created(), check_freshness()
  - src/wssec/nonce_cache.rs with RotatingNonceCache, check_and_insert()
  - src/wssec/mod.rs with public re-exports

affects: [06, 09]

tech-stack:
  added: []
  patterns:
    - NsReader streaming XML parse — attribute-based namespace collection for envelope ns_bindings
    - Two-bucket rotating cache pattern for O(1) nonce replay detection with bounded memory
    - TDD — tests written and verified alongside implementation

key-files:
  created: []
  modified:
    - src/envelope.rs
    - src/wssec/timestamp.rs
    - src/wssec/nonce_cache.rs
    - src/wssec/mod.rs

key-decisions:
  - "Namespace re-emission on body fragment uses attribute inspection not NsReader.resolver().bindings() — quick-xml 0.39 NsReader does not expose a bindings() iterator on the resolver at the Start event level; collecting xmlns:* attributes from the Envelope element directly is equivalent and works correctly"
  - "force_rotate() test helper added to RotatingNonceCache under #[cfg(test)] — avoids sleeping in tests while still verifying cross-bucket replay detection"

duration: 3min
completed: "2026-04-03"
---

# Phase 1 Plan 4: Envelope Parsing and WS-Security Infrastructure Summary

**SOAP 1.2/1.1 envelope parser with namespace context propagation, timestamp freshness validator, and two-bucket rotating nonce replay cache — 22 new tests, 69 total passing**

## Performance

- **Duration:** ~3 min
- **Started:** 2026-04-03T18:11:54Z
- **Completed:** 2026-04-03T18:14:52Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments

- parse_envelope() streaming parse using NsReader: extracts Header children as Vec<Bytes> and Body first child element as self-contained Bytes with ancestor xmlns declarations re-emitted on the fragment root
- detect_soap_version() maps application/soap+xml to Soap12, text/xml to Soap11, everything else to VersionMismatch fault
- serialize_envelope() wraps handler output bytes in correct SOAP namespace Envelope+Body
- response_content_type() returns appropriate Content-Type for SOAP 1.2 and 1.1 responses
- check_freshness() validates wsu:Created timestamps against a tolerance window (default 300s), rejecting expired and future timestamps
- parse_created() parses RFC 3339 / ISO 8601 strings with graceful error handling
- RotatingNonceCache with two-bucket rotation every half_window_secs, detecting replays across bucket boundaries and dropping old nonces after two rotations
- wssec/mod.rs updated with public exports; username_token stub maintained for plan 06

## Task Commits

1. **Task 1: SOAP envelope parse and serialize** — `f9b2a92` (feat)
2. **Task 2: WS-Security timestamp and nonce cache** — `30fb771` (feat)

## Files Created/Modified

- `src/envelope.rs` — ParsedEnvelope, parse_envelope(), detect_soap_version(), serialize_envelope(), response_content_type(), 10 tests
- `src/wssec/timestamp.rs` — parse_created(), check_freshness(), 7 tests
- `src/wssec/nonce_cache.rs` — RotatingNonceCache, check_and_insert(), force_rotate() test helper, 5 tests
- `src/wssec/mod.rs` — public re-exports with username_token stub

## Decisions Made

- Namespace re-emission uses attribute inspection on the Envelope element rather than NsReader.resolver().bindings(). quick-xml 0.39 NsReader does not surface a bindings iterator at the Start event; parsing xmlns:* attributes directly from the Envelope element achieves the same result and is reliable.
- force_rotate() test helper added under #[cfg(test)] on RotatingNonceCache to enable deterministic bucket rotation testing without time.sleep() calls.

## Deviations from Plan

### Auto-fixed Issues

None — plan executed exactly as written.

## Self-Check: PASSED

- FOUND: src/envelope.rs (parse_envelope, NsReader)
- FOUND: src/wssec/timestamp.rs (check_freshness)
- FOUND: src/wssec/nonce_cache.rs (RotatingNonceCache)
- FOUND: src/wssec/mod.rs (pub re-exports)
- FOUND commit: f9b2a92
- FOUND commit: 30fb771
- cargo check: PASSED
- 69 tests (22 new): all PASSED
