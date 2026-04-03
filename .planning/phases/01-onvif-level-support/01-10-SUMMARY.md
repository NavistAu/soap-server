---
phase: 01-onvif-level-support
plan: "10"
subsystem: testing
tags: [integration-test, onvif, wsdl, wssec, axum, soap12, fixtures]

requires:
  - phase: 01-onvif-level-support/01-09
    provides: ServerBuilder, SoapService::into_router(), WS-Security pipeline, WSDL GET serving
  - phase: 01-onvif-level-support/01-06
    provides: compute_digest, validate_username_token
  - phase: 01-onvif-level-support/01-07
    provides: resolve_wsdl, WsdlLoader trait, rewrite_wsdl_address

provides:
  - tests/onvif_integration_test.rs — 7 integration tests covering all 5 Phase 1 roadmap success criteria with real ONVIF devicemgmt.wsdl fixture
  - ServerBuilder::default_handler() — catch-all for unregistered WSDL operations
  - ServerBuilder::from_wsdl_bytes_with_loader() — custom loader for external imports
  - FileWsdlLoader — filesystem-based WSDL/XSD import resolver with normalize_path()
  - Tolerant XSD resolver — unknown external type/element refs return empty ComplexType (not error)
  - Header namespace re-emission — envelope xmlns bindings re-emitted on extracted header child fragments

affects:
  - onvif-server (downstream consumer) — Phase 1 is complete and ready

tech-stack:
  added: []
  patterns:
    - FixtureLoader pattern for integration tests — maps external WSDL import paths to local fixture files by basename
    - default_handler pattern — catch-all for large WSDLs (100+ operations) where only a subset needs real handlers
    - from_wsdl_bytes_with_loader() — custom loader injection for test environments with file fixtures
    - format_wsu_created() helper — stdlib-only current-time formatting for WS-Security test vectors
    - Tolerant XSD resolution — unresolvable external refs produce Empty ComplexType (forward-compat with partial schemas)

key-files:
  created:
    - tests/onvif_integration_test.rs
  modified:
    - src/server.rs
    - src/dispatch.rs
    - src/envelope.rs
    - src/xsd/resolver.rs
    - src/lib.rs

key-decisions:
  - "ServerBuilder::default_handler() required for large multi-operation WSDLs — build() fails with UnregisteredOperation for every unhandled op without it"
  - "XSD resolver tolerant unknown refs — external schemas (wsn/b-2, xop/include) are never fetched; unresolvable type/element refs return Empty ComplexType rather than error"
  - "Header namespace re-emission bug fix — collect_header_children() must re-emit envelope xmlns:* bindings on each child fragment root for wsse: prefix resolution to work in UsernameToken parser"
  - "FixtureLoader maps by basename — real ONVIF relative path ../../../ver10/schema/onvif.xsd resolved to tests/fixtures/onvif.xsd by extracting last path component"
  - "format_wsu_created() uses stdlib-only integer arithmetic — avoids chrono dependency in test code; computes ISO 8601 timestamp from UNIX epoch for WS-Security Created field"

requirements-completed: [WSDL-01, WSDL-02, WSDL-03, WSDL-04, WSDL-05]

duration: 20min
completed: 2026-04-03
---

# Phase 01 Plan 10: ONVIF End-to-End Integration Tests Summary

**Real ONVIF devicemgmt.wsdl (with onvif.xsd/common.xsd fixtures) loaded and serving SOAP 1.2 with WS-Security, validated by 7 automated tests covering all 5 Phase 1 roadmap success criteria**

## Performance

- **Duration:** ~20 min
- **Started:** 2026-04-03T18:54:00Z
- **Completed:** 2026-04-03T19:06:50Z
- **Tasks:** 1 (Task 2 is checkpoint:human-verify — awaiting approval)
- **Files modified:** 6

## Accomplishments

- `tests/onvif_integration_test.rs`: 7 integration tests using real ONVIF `devicemgmt.wsdl` + `onvif.xsd` + `common.xsd` fixtures
- All 5 Phase 1 roadmap success criteria covered by automated tests:
  1. Multi-file WSDL loads without panic — `test_onvif_wsdl_loads`
  2. SOAP 1.2 dispatch to correct handler — `test_soap12_dispatch`
  3. WS-Security: valid credentials accepted (`test_wssec_valid_credentials_accepted`), wrong password rejected (`test_wssec_wrong_password_rejected`), auth bypass works (`test_auth_bypass_get_system_date`)
  4. GET ?wsdl returns rewritten WSDL — `test_wsdl_serving`
  5. axum Router composes cleanly — `test_router_composition`
- Full test suite: 175 tests pass (162 unit + 6 existing integration + 7 new ONVIF integration)

## Task Commits

1. **Task 1: ONVIF end-to-end integration tests** - `93e0278` (feat)

**Plan metadata:** (docs commit follows after Task 2 checkpoint approval)

## Files Created/Modified

- `tests/onvif_integration_test.rs` — 7 integration tests, FixtureLoader, format_wsu_created() helper
- `src/server.rs` — ServerBuilder::default_handler(), from_wsdl_bytes_with_loader(), FileWsdlLoader, normalize_path(), custom_loader field
- `src/dispatch.rs` — build_dispatch_table() optional default_handler param, DispatchTable.default_handler field, updated 9 test call sites
- `src/envelope.rs` — collect_header_children() namespace re-emission fix
- `src/xsd/resolver.rs` — tolerant unknown type ref + tolerant unknown element ref
- `src/lib.rs` — pub re-exports: FileWsdlLoader, WsdlLoader, WsdlError, compute_digest

## Decisions Made

- `ServerBuilder::default_handler()` required for large multi-operation WSDLs — the real ONVIF devicemgmt.wsdl has ~100 operations; without a catch-all, build() fails for every unregistered op
- `XSD resolver tolerant unknown refs` — external schemas (wsn/b-2, xop/include) are never fetched; unresolvable type/element refs return `Empty ComplexType` rather than error, consistent with the "unknown type is not an error" principle from prior plans
- `Header namespace re-emission bug` — `collect_header_children()` was not re-emitting envelope xmlns bindings on extracted header child bytes. The `_ns_bindings` parameter was intentionally unused. This caused `parse_username_token` to fail with "Missing UsernameToken" because the `wsse:` prefix had no binding in the fragment. Fixed by emitting inherited bindings on each depth=0 child
- `FixtureLoader maps by basename` — the real ONVIF WSDL uses relative paths like `../../../ver10/schema/onvif.xsd`; fixture files are all in `tests/fixtures/`; a basename-based lookup is simpler and more robust than path normalization
- `format_wsu_created() stdlib-only` — avoids bringing chrono into test code; Gregorian calendar conversion via integer arithmetic (civil epoch algorithm)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] ServerBuilder::default_handler() and from_wsdl_bytes_with_loader()**
- **Found during:** Task 1 (integration test implementation)
- **Issue:** The real ONVIF WSDL has ~100 operations. The existing API requires registering all operations as handlers; no catch-all existed. Without one, building a service from the real WSDL would fail with `UnregisteredOperation`.
- **Fix:** Added `default_handler` field to `ServerBuilder`, `default_handler()` builder method, `from_wsdl_bytes_with_loader()` for custom loader injection, and updated `build_dispatch_table()` to accept an `Option<Arc<dyn SoapHandler>>` catch-all
- **Files modified:** src/server.rs, src/dispatch.rs
- **Committed in:** 93e0278

**2. [Rule 1 - Bug] XSD resolver failed with UnknownRef on external schema types**
- **Found during:** Task 1 (first test run)
- **Issue:** `resolve_named_type()` returned `Err(SchemaError::UnknownRef)` for types from external schemas (e.g., `{http://docs.oasis-open.org/wsn/b-2}FilterType`). These are from optional extension schemas not bundled in fixtures.
- **Fix:** Changed `resolve_named_type()` to return `Ok(ComplexType { content: Empty })` when type not found — consistent with existing "unknown type is not an error" dispatch policy
- **Files modified:** src/xsd/resolver.rs
- **Committed in:** 93e0278

**3. [Rule 1 - Bug] XSD resolver failed with UnknownRef on external element refs**
- **Found during:** Task 1 (second test run, after type ref fix)
- **Issue:** `resolve_element_list()` returned `Err(SchemaError::UnknownRef)` for `xs:element ref=` pointing to elements from external schemas (e.g., `xop:Include`)
- **Fix:** Changed unknown element ref to `continue` (skip) instead of returning error
- **Files modified:** src/xsd/resolver.rs
- **Committed in:** 93e0278

**4. [Rule 1 - Bug] Header namespace bindings not re-emitted on extracted header child fragments**
- **Found during:** Task 1 (WS-Security auth test failure with "Missing UsernameToken")
- **Issue:** `collect_header_children()` extracted the wsse:Security element bytes without re-emitting the `xmlns:wsse` and `xmlns:wsu` declarations from the Envelope element. `parse_username_token()` could not resolve the `wsse:` prefix, so `in_username_token` was never set, and validation always returned "Missing UsernameToken"
- **Fix:** Changed `_ns_bindings` to `ns_bindings` (removed underscore) and added logic to re-emit inherited namespace bindings on each depth=0 child element's start tag, skipping bindings already declared on the element itself
- **Files modified:** src/envelope.rs
- **Committed in:** 93e0278

---

**Total deviations:** 4 auto-fixed (2x Rule 1 - Bug in XSD resolver, 1x Rule 1 - Bug in envelope.rs, 1x Rule 2 - Missing Critical in ServerBuilder)
**Impact on plan:** All four were blocking the integration tests from working with the real ONVIF fixtures. No scope creep — all fixes are correctness requirements for ONVIF WSDL loading.

## Issues Encountered

- WS-Security timestamp check initially failed because test used hardcoded `created = "2026-04-03T12:00:00.000Z"`. The server uses `Utc::now()` for freshness checking with ±300s tolerance. Fixed by computing `created` from current system time minus 1 second using a stdlib-only `format_wsu_created()` helper.

## Next Phase Readiness

- Phase 1 (ONVIF-Level Support) complete pending Task 2 checkpoint approval
- All 5 Phase 1 roadmap success criteria verified by automated tests
- `ServerBuilder::from_wsdl_bytes(wsdl).handler(...).auth(...).build()?.into_router()` public API ready for consumption by onvif-server
- 175/175 tests pass

## Self-Check: PASSED

- tests/onvif_integration_test.rs: FOUND
- src/server.rs FileWsdlLoader: FOUND
- src/dispatch.rs default_handler: FOUND
- src/envelope.rs ns re-emission: FOUND
- src/xsd/resolver.rs tolerant refs: FOUND
- Commit 93e0278 (Task 1): FOUND
- cargo test all pass: 175/175

---
*Phase: 01-onvif-level-support*
*Completed: 2026-04-03*
