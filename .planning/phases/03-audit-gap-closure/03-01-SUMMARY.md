---
phase: 03-audit-gap-closure
plan: "01"
subsystem: api
tags: [axum, wsdl, soap, rust, dispatch, wssec]

# Dependency graph
requires:
  - phase: 02-full-spec-compliance
    provides: multi-service routing (SoapServiceRoute), DispatchTable, RotatingNonceCache, validate_username_token
provides:
  - WSDL GET handler registered in multi-service into_router() branch
  - Public re-exports for RotatingNonceCache, DispatchTable, build_dispatch_table, validate_username_token
  - Stale TODO comments removed from fault.rs and envelope.rs
affects: [consumers of soap-server crate, multi-service deployments]

# Tech tracking
tech-stack:
  added: []
  patterns: [axum per-path state with separate GET/POST states via chained route() calls]

key-files:
  created: []
  modified:
    - src/server.rs
    - src/lib.rs
    - src/fault.rs
    - src/envelope.rs

key-decisions:
  - "Multi-service WSDL GET uses separate router.route(path, get(wsdl_get_handler).with_state(state.clone())) chained after POST route — axum allows multiple route() calls on the same path with different states"
  - "dispatch module promoted from pub(crate) to pub to enable DispatchTable re-export from crate root"

patterns-established:
  - "Pattern: axum allows chaining separate .route() calls on the same path with different states for GET vs POST handlers"

requirements-completed: [WSDL-04, DSP-06]

# Metrics
duration: 8min
completed: 2026-04-05
---

# Phase 3 Plan 01: Audit Gap Closure — WSDL GET + Public API Summary

**Multi-service WSDL GET fixed via chained axum GET route with Arc<SoapService> state; four internal types promoted to crate-root public API; two stale TODO comments replaced with doc comments**

## Performance

- **Duration:** 8 min
- **Started:** 2026-04-05T06:20:00Z
- **Completed:** 2026-04-05T06:28:00Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments
- GET ?wsdl now returns WSDL XML in multi-service mode (was returning 405 Method Not Allowed)
- RotatingNonceCache, DispatchTable, build_dispatch_table, validate_username_token now importable from crate root
- src/fault.rs and src/envelope.rs line 1 stale TODO comments replaced with proper doc comments
- All 204 tests pass (186 unit + 11 integration + 7 ONVIF integration)

## Task Commits

Each task was committed atomically:

1. **Task 1: Add WSDL GET handler to multi-service into_router() branch** - `2931cc1` (feat)
2. **Task 2: Promote internal types to public API and remove stale TODOs** - `00f6e5e` (feat)

## Files Created/Modified
- `src/server.rs` - Added `get` import from axum::routing; registered wsdl_get_handler GET route per service path in multi-service branch
- `src/lib.rs` - Changed dispatch from pub(crate) to pub; added pub use for DispatchTable, build_dispatch_table, RotatingNonceCache, validate_username_token
- `src/fault.rs` - Replaced `// TODO: SOAP 1.2 fault generation` with `//! SOAP fault types and serialization for SOAP 1.1 and 1.2.`
- `src/envelope.rs` - Replaced `// TODO: SOAP 1.2 envelope parsing and serialization` with `//! SOAP envelope parsing and serialization for SOAP 1.1 and 1.2.`

## Decisions Made
- Multi-service WSDL GET uses a separate `router.route(path, get(wsdl_get_handler).with_state(state.clone()))` call chained after the POST route for each service path. Axum allows multiple route() calls on the same path with different method+state combinations, so no new handler variant was needed.
- dispatch module promoted from `pub(crate)` to `pub` to enable the `DispatchTable` and `build_dispatch_table` re-exports from the crate root without re-exporting the entire module's internals.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- All v1.0 audit gap closure items for this plan are resolved
- Public API surface is now complete for crate consumers
- Multi-service WSDL GET works correctly

---
*Phase: 03-audit-gap-closure*
*Completed: 2026-04-05*
