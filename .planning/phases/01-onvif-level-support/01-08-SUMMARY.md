---
phase: 01-onvif-level-support
plan: "08"
subsystem: dispatch
tags: [dispatch, routing, qname, soap-action, xsd-validation, quick-xml, auth]

requires:
  - phase: 01-onvif-level-support/01-07
    provides: resolve_wsdl(), ResolvedWsdl, TypeRegistry — consumed by build_dispatch_table to build routing entries
  - phase: 01-onvif-level-support/01-02
    provides: SoapHandler trait, FnHandler — registered handlers stored in DispatchEntry
  - phase: 01-onvif-level-support/01-02
    provides: SoapFault, FaultCode::Sender, action_not_supported() — returned on routing miss

provides:
  - DispatchEntry — handler Arc + auth_required flag + input_type Option<QName>
  - DispatchTable — by_element HashMap<QName, DispatchEntry> + by_action HashMap<String, DispatchEntry>
  - build_dispatch_table() — builds table from ResolvedWsdl + handlers + auth_bypass set; fast-fails at startup
  - route() — O(1) body-QName lookup with SOAPAction fallback; returns SoapFault::Sender on miss
  - validate_request() — XSD-11 structural validation: checks required element presence via quick-xml streaming

affects:
  - 01-09 request pipeline (consumes DispatchTable.route() per request)
  - 01-09 security interceptor (reads DispatchEntry.auth_required before invoking handler)

tech-stack:
  added: []
  patterns:
    - HashMap<QName, DispatchEntry> for O(1) by-element routing; HashMap<String, DispatchEntry> for SOAPAction fallback
    - Arc<dyn SoapHandler> stored per entry — shared ownership, zero-copy dispatch
    - build-time fast-fail for unregistered/unknown operations (DispatchError at startup, not request time)
    - quick-xml Reader::from_reader for streaming XML child-element enumeration in validate_request
    - auth_bypass HashSet<String> at build time marks entries that skip authentication

key-files:
  created:
    - src/dispatch.rs
  modified: []

key-decisions:
  - "DispatchEntry.auth_required set at build time from auth_bypass HashSet — avoids per-request set lookup; security interceptor reads a bool"
  - "validate_request skips validation silently when input_type is None or qname not in registry — unknown type is not an error (forward-compat with partial WSDLs)"
  - "by_element and by_action are separate HashMaps not a single enum — route() checks element first, action second; entries are independent (same handler stored twice by Arc clone)"
  - "build_dispatch_table iterates services->ports->bindings->ops; falls back to all bindings if no services defined — handles partial WSDLs used in tests"

requirements-completed: [DSP-01, DSP-02, DSP-03, DSP-04, XSD-11]

duration: 5min
completed: 2026-04-03
---

# Phase 01 Plan 08: Dispatch Table and XSD Payload Validation Summary

**O(1) QName-to-handler routing table with SOAPAction fallback, build-time operation validation, and structural XSD-11 payload validation via quick-xml streaming**

## Performance

- **Duration:** ~5 min
- **Started:** 2026-04-03T18:39:35Z
- **Completed:** 2026-04-03T18:43:50Z
- **Tasks:** 1
- **Files modified:** 1

## Accomplishments

- `DispatchTable` provides O(1) routing by body element QName (`by_element: HashMap<QName, DispatchEntry>`) with `by_action: HashMap<String, DispatchEntry>` SOAPAction fallback
- `build_dispatch_table()` fast-fails at startup with `DispatchError::UnregisteredOperation` if any WSDL operation has no handler, and `DispatchError::UnknownOperation` if any registered handler has no WSDL operation
- `route()` returns `Err(SoapFault::sender(...))` with `FaultCode::Sender` when neither element QName nor SOAPAction matches
- `DispatchEntry.auth_required` is set per-entry at build time from the `auth_bypass` set — enables the security interceptor to check a plain `bool` per request
- `validate_request()` implements XSD-11 structural validation: quick-xml streams `body_bytes`, collects direct child element local names, then checks all `min_occurs > 0` elements in the resolved `ComplexType` are present; unknown types are silently skipped
- 14 unit tests cover all routing paths, auth bypass, validation with required/optional elements, and error cases

## Task Commits

1. **Task 1: Dispatch table and XSD payload validation** - `573b4db` (feat)

**Plan metadata:** (docs commit follows)

## Files Created/Modified

- `src/dispatch.rs` — DispatchEntry, DispatchTable, build_dispatch_table(), route(), validate_request(), 14 unit tests

## Decisions Made

- `DispatchEntry.auth_required` is a pre-computed `bool` (not a runtime lookup) — the security interceptor reads it without touching the bypass set
- `validate_request` returns `Ok(())` when the input type QName is absent or not in the registry — this is correct behavior for partial WSDLs and forward compatibility
- `by_element` and `by_action` store independent `DispatchEntry` values (handler `Arc` is cloned at build time) — simpler than storing a single entry with two index keys

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Added manual Debug impl for DispatchEntry**
- **Found during:** Task 1 (compile error — `unwrap_err()` requires `Debug`)
- **Issue:** `DispatchEntry` could not derive `Debug` because `Arc<dyn SoapHandler>` does not implement `Debug`. The test helper `result.unwrap_err()` requires `T: Debug`.
- **Fix:** Added `impl std::fmt::Debug for DispatchEntry` using `finish_non_exhaustive()` to omit the handler field while emitting `auth_required` and `input_type`.
- **Files modified:** src/dispatch.rs
- **Committed in:** 573b4db

---

**Total deviations:** 1 auto-fixed (1x Rule 1 - Bug)
**Impact on plan:** Minimal — one-line fix; no scope change.

## Issues Encountered

None beyond the Debug impl above.

## Next Phase Readiness

- `DispatchTable` is ready for consumption by the request pipeline (plan 09)
- `DispatchEntry.auth_required` enables the security interceptor to gate requests without additional lookups
- `validate_request()` is ready to be called before handler invocation in the pipeline
- All 156 tests pass (14 dispatch + 142 prior)

## Self-Check: PASSED

- src/dispatch.rs: FOUND
- Commit 573b4db (Task 1): FOUND
- cargo check: exits 0
- 14/14 dispatch:: tests pass
- 156/156 total tests pass

---
*Phase: 01-onvif-level-support*
*Completed: 2026-04-03*
