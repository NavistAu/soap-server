---
phase: 01-onvif-level-support
plan: "09"
subsystem: server
tags: [axum, soap, server-builder, request-pipeline, wssec, wsdl-serving, integration-test]

requires:
  - phase: 01-onvif-level-support/01-08
    provides: DispatchTable, build_dispatch_table(), route(), validate_request(), DispatchEntry.auth_required
  - phase: 01-onvif-level-support/01-06
    provides: validate_username_token(), RotatingNonceCache, WS-Security UsernameToken validation
  - phase: 01-onvif-level-support/01-07
    provides: resolve_wsdl(), ResolvedWsdl, rewrite_wsdl_address(), WsdlLoader trait
  - phase: 01-onvif-level-support/01-04
    provides: parse_envelope(), serialize_envelope(), detect_soap_version(), ParsedEnvelope
  - phase: 01-onvif-level-support/01-02
    provides: SoapHandler trait, FnHandler, SoapFault, FaultCode

provides:
  - ServerBuilder — fluent builder API for constructing SoapService
  - SoapService::into_router() — returns composable axum::Router
  - Full SOAP 1.2 request pipeline (version detect -> parse -> dispatch -> auth -> validate -> handle)
  - WSDL GET serving with soap:address rewrite from Host header
  - BuildError — typed error for startup misconfiguration
  - Public re-exports in lib.rs: ServerBuilder, SoapService, SoapHandler, FnHandler, SoapFault, FaultCode, BuildError

affects:
  - Consumers of the crate (onvif-server and any downstream users)

tech-stack:
  added:
    - serde 1.x with derive feature (for axum Query extractor on WsdlQuery struct)
    - chrono clock feature (enables Utc::now() for WS-Security timestamp validation)
  patterns:
    - Arc<SoapService> as axum State — zero-copy shared ownership per request
    - Auth gate as bool per DispatchEntry — no per-request set lookup
    - SOAP fault always serialized to 500 + application/soap+xml (FLT-03/HTTP-03)
    - TestServer::new().bytes().content_type() for axum-test SOAP integration tests
    - NoOpLoader pattern for embedded/self-contained WSDL (no external imports)

key-files:
  created:
    - src/server.rs
    - tests/integration_test.rs
  modified:
    - src/lib.rs
    - Cargo.toml

key-decisions:
  - "TestServer::new().bytes().content_type() — axum-test .text() overrides content-type with text/plain; must use .bytes() to preserve application/soap+xml"
  - "NoOpLoader returns WsdlError::MalformedXml for external imports — embedded-mode WSDLs are self-contained; external imports unsupported in this builder"
  - "SoapService has manual Debug impl (not derive) — auth_fn and nonce_cache fields cannot derive Debug"
  - "chrono clock feature added to enable Utc::now() — was previously missing from feature flags"

requirements-completed: [SEC-01, SEC-06, SEC-07, HTTP-01, HTTP-02, HTTP-03, HTTP-04]

duration: 7min
completed: 2026-04-03
---

# Phase 01 Plan 09: ServerBuilder and axum Integration Layer Summary

**ServerBuilder fluent API composes WSDL resolution, WS-Security auth, dispatch, and handler invocation into a composable axum::Router via SoapService::into_router()**

## Performance

- **Duration:** ~7 min
- **Started:** 2026-04-03T18:46:25Z
- **Completed:** 2026-04-03T18:53:39Z
- **Tasks:** 1
- **Files modified:** 4

## Accomplishments

- `ServerBuilder::from_wsdl_bytes/from_wsdl_file` with fluent `.handler()`, `.auth()`, `.auth_bypass()`, `.path()` API
- `SoapService::into_router()` returns an `axum::Router` composable with `Router::merge()`
- Full SOAP 1.2 request pipeline: `detect_soap_version` -> `parse_envelope` -> `extract_body_qname` -> `dispatch::route` -> WS-Security validation (if `auth_required`) -> `validate_request` -> handler invocation -> `serialize_envelope`
- WSDL GET handler: `?wsdl` query returns `rewrite_wsdl_address`-modified bytes with `Host`/`X-Forwarded-Host`-derived address; absent `?wsdl` returns 404
- All SOAP faults return HTTP 500 with `application/soap+xml; charset=utf-8` content-type (per FLT-03/HTTP-03)
- 6 integration tests in `tests/integration_test.rs` covering the complete pipeline via axum-test
- `lib.rs` updated with clean public re-exports: `ServerBuilder`, `SoapService`, `BuildError`, `SoapHandler`, `FnHandler`, `SoapFault`, `FaultCode`

## Task Commits

1. **Task 1: ServerBuilder, SoapService, and request pipeline** - `75b49a4` (feat)

**Plan metadata:** (docs commit follows)

## Files Created/Modified

- `src/server.rs` — ServerBuilder, SoapService, BuildError, NoOpLoader, soap_post_handler, wsdl_get_handler, extract_body_qname, find_security_header, 5 unit tests
- `tests/integration_test.rs` — 6 integration tests exercising full pipeline via axum-test
- `src/lib.rs` — pub use re-exports for all public API types
- `Cargo.toml` — serde with derive feature + chrono clock feature added

## Decisions Made

- `TestServer::new().bytes().content_type()` — axum-test 20.x's `.text()` method overrides the content-type header to `text/plain`, which breaks SOAP version detection. Must use `.bytes()` to preserve `application/soap+xml`.
- `NoOpLoader` returns `WsdlError::MalformedXml` for external WSDL imports — the `from_wsdl_bytes` path is intended for embedded/self-contained WSDLs only.
- `SoapService` uses manual `Debug` impl via `finish_non_exhaustive()` — `Arc<dyn Fn>` and `tokio::sync::Mutex` cannot derive `Debug`.
- `chrono clock` feature was missing from Cargo.toml — required to call `Utc::now()` in the auth interceptor.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] axum-test .text() overrides Content-Type header**
- **Found during:** Task 1 (integration test RED phase)
- **Issue:** axum-test's `.text(body)` sets `Content-Type: text/plain`, overriding `.content_type("application/soap+xml")` — causing `detect_soap_version` to return `VersionMismatch` fault
- **Fix:** Changed all POST calls to `.bytes(Bytes::from(body.into_bytes())).content_type("application/soap+xml")`
- **Files modified:** tests/integration_test.rs
- **Committed in:** 75b49a4

**2. [Rule 3 - Blocking] Missing chrono clock feature for Utc::now()**
- **Found during:** Task 1 (compile error)
- **Issue:** `Utc::now()` not available — chrono was configured with `features = ["std"]` only, which excludes the `clock` feature
- **Fix:** Added `"clock"` to chrono features in Cargo.toml
- **Files modified:** Cargo.toml
- **Committed in:** 75b49a4

**3. [Rule 3 - Blocking] SoapService missing Debug for unit test unwrap_err()**
- **Found during:** Task 1 (compile error)
- **Issue:** `Result<SoapService, BuildError>::unwrap_err()` requires `T: Debug`; SoapService can't derive Debug due to function pointer and Mutex fields
- **Fix:** Added manual `impl std::fmt::Debug for SoapService` using `finish_non_exhaustive()`
- **Files modified:** src/server.rs
- **Committed in:** 75b49a4

---

**Total deviations:** 3 auto-fixed (3x Rule 3 - Blocking)
**Impact on plan:** All three were discovered during compile/test and resolved inline. No scope change.

## Issues Encountered

None beyond the three auto-fixed blocking issues above.

## Next Phase Readiness

- `ServerBuilder::from_wsdl_bytes(bytes).handler(...).auth(...).build()?.into_router()` is the complete public API — ready for consumption by onvif-server
- All 168 tests pass (162 unit + 6 integration)
- Phase 1 (ONVIF-Level Support) is now complete — all 9 plans executed

## Self-Check: PASSED

- src/server.rs: FOUND
- tests/integration_test.rs: FOUND
- src/lib.rs: FOUND (pub use re-exports confirmed)
- Commit 75b49a4 (Task 1): FOUND
- cargo check: exits 0
- 162/162 unit tests pass
- 6/6 integration tests pass
- 168/168 total tests pass

---
*Phase: 01-onvif-level-support*
*Completed: 2026-04-03*
