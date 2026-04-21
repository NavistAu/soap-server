---
phase: 01-onvif-level-support
plan: "01"
subsystem: infra
tags: [rust, cargo, roxmltree, quick-xml, axum, tokio, onvif, wsdl, xsd]

requires: []
provides:
  - Cargo.toml with all locked dependencies for the soap-server crate
  - src/lib.rs declaring all top-level module paths (dispatch, envelope, fault, handler, server, wsdl, wssec, xsd)
  - Full module stub hierarchy (wsdl/, xsd/, wssec/ subdirectories with their sub-modules)
  - tests/fixtures/devicemgmt.wsdl — real ONVIF Device Management WSDL from onvif.org
  - tests/fixtures/onvif.xsd — real ONVIF core type schema from onvif.org
  - tests/fixtures/common.xsd — real ONVIF common types schema from onvif.org
affects: [02, 03, 04, 05, 06, 07, 08, 09, 10]

tech-stack:
  added:
    - roxmltree 0.21 (WSDL/XSD DOM parsing at startup)
    - quick-xml 0.39 with async-tokio feature (per-request streaming)
    - axum 0.8 (HTTP framework)
    - tokio 1 with sync + full features (async runtime)
    - sha1 0.11 (WS-Security PasswordDigest)
    - base64 0.22 (WS-Security encoding)
    - chrono 0.4 (WS-Security timestamp)
    - thiserror 2 (error types)
    - bytes 1 + http-body-util 0.1 (body handling)
    - axum-test 20 (dev, integration testing)
  patterns:
    - Two-pass parse pattern — parse then resolve (handles forward references in ONVIF WSDLs)
    - Module hierarchy mirrors SOAP protocol layers (envelope, dispatch, fault, handler, server)
    - WS-Security in dedicated wssec module (username_token, nonce_cache, timestamp)

key-files:
  created:
    - Cargo.toml
    - .tool-versions
    - src/lib.rs
    - src/dispatch.rs
    - src/envelope.rs
    - src/fault.rs
    - src/handler.rs
    - src/server.rs
    - src/wsdl/mod.rs + parser.rs + resolver.rs + definitions.rs
    - src/xsd/mod.rs + parser.rs + resolver.rs + types.rs + elements.rs
    - src/wssec/mod.rs + username_token.rs + nonce_cache.rs + timestamp.rs
    - tests/fixtures/devicemgmt.wsdl
    - tests/fixtures/onvif.xsd
    - tests/fixtures/common.xsd
  modified: []

key-decisions:
  - "axum-test uses version 20.x not 0.16 — crate switched to calendar-style versioning"
  - ".tool-versions pins Rust 1.85.1 — required for edition2024 support (sha1 dep tree pulls cpufeatures 0.3 which requires it)"
  - "Real ONVIF fixtures downloaded from onvif.org rather than hand-crafted — provides canonical correctness basis for all future plans"

patterns-established:
  - "Module stubs pattern: all modules declared in lib.rs with stub files even before implementation"
  - "Fixture-driven development: real vendor schemas bundled in tests/fixtures/ for integration tests"

requirements-completed:
  - XSD-01

duration: 3min
completed: "2026-04-03"
---

# Phase 1 Plan 1: Crate Bootstrap Summary

**soap-server crate scaffolded with all locked dependencies, full module skeleton, and real ONVIF WSDL/XSD test fixtures downloaded from onvif.org**

## Performance

- **Duration:** 3 min
- **Started:** 2026-04-03T17:59:11Z
- **Completed:** 2026-04-03T18:02:20Z
- **Tasks:** 2
- **Files modified:** 26

## Accomplishments

- Cargo.toml with all 10 production dependencies and 2 dev dependencies locked to exact versions
- 22-file module stub hierarchy covering all protocol layers (envelope, dispatch, fault, handler, server, wsdl, xsd, wssec)
- Real ONVIF Device Management WSDL and associated XSD schemas from onvif.org bundled as test fixtures
- `cargo check` passes clean with Rust 1.85.1

## Task Commits

Each task was committed atomically:

1. **Task 1: Create Cargo.toml with all locked dependencies** - `9d86835` (chore)
2. **Task 2: Create src/lib.rs module skeleton and ONVIF test fixtures** - `69ebc86` (feat)

**Plan metadata:** `aaa3bf7` (docs: complete crate bootstrap plan)

## Files Created/Modified

- `Cargo.toml` - Library crate manifest with all dependencies
- `.tool-versions` - Pins Rust 1.85.1 for edition2024 compatibility
- `src/lib.rs` - Crate root declaring 8 pub modules
- `src/dispatch.rs` - Stub: SOAP body dispatch
- `src/envelope.rs` - Stub: SOAP 1.2 envelope parsing/serialization
- `src/fault.rs` - Stub: SOAP 1.2 fault generation
- `src/handler.rs` - Stub: Raw handler trait
- `src/server.rs` - Stub: axum Router integration
- `src/wsdl/` - 4 stubs: mod, parser, resolver, definitions
- `src/xsd/` - 5 stubs: mod, parser, resolver, types, elements
- `src/wssec/` - 4 stubs: mod, username_token, nonce_cache, timestamp
- `tests/fixtures/devicemgmt.wsdl` - Real ONVIF Device Management WSDL (from onvif.org)
- `tests/fixtures/onvif.xsd` - Real ONVIF core type schema (from onvif.org)
- `tests/fixtures/common.xsd` - Real ONVIF common types schema (from onvif.org)

## Decisions Made

- `axum-test` uses version 20.x not 0.16 — the crate switched to calendar-style versioning. Fixed to "20".
- `.tool-versions` needed to pin Rust 1.85.1 — the global mise config had 1.79.0 which lacks edition2024 support required by `cpufeatures 0.3.0` in the sha1 dep tree.
- Downloaded real ONVIF fixtures from onvif.org rather than creating hand-crafted ones — provides canonical correctness basis for all integration tests throughout the phase.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] axum-test version 0.16 does not exist**
- **Found during:** Task 1 (Cargo.toml creation)
- **Issue:** axum-test crate uses calendar versioning (currently 20.x), not semver 0.x. Version "0.16" produces "failed to select a version" error.
- **Fix:** Changed dev-dependency to `axum-test = "20"`
- **Files modified:** Cargo.toml
- **Verification:** cargo check resolves the dependency without error
- **Committed in:** 9d86835 (Task 1 commit)

**2. [Rule 3 - Blocker] Rust 1.79.0 incompatible with cpufeatures 0.3.0 (edition2024)**
- **Found during:** Task 1 (cargo check after fixing axum-test)
- **Issue:** axum-test 20 → cpufeatures 0.3.0 which requires `edition2024` Cargo feature, not stable until Rust 1.85. Global mise config had Rust 1.79.0.
- **Fix:** Created `.tool-versions` pinning `rust 1.85.1` (already installed via mise)
- **Files modified:** .tool-versions (new file)
- **Verification:** `mise exec -- cargo check` exits 0 with Rust 1.85.1
- **Committed in:** 9d86835 (Task 1 commit)

---

**Total deviations:** 2 auto-fixed (1 bug, 1 blocker)
**Impact on plan:** Both required for the crate to compile at all. No scope creep.

## Issues Encountered

None beyond the auto-fixed deviations above.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Crate compiles from scratch with all dependencies resolved
- All module paths declared — subsequent plans can implement each module independently
- Real ONVIF WSDL/XSD fixtures ready for driving integration tests throughout the phase
- No blockers — any of the 8 modules can now be implemented in subsequent plans

## Self-Check: PASSED

- FOUND: Cargo.toml
- FOUND: .tool-versions
- FOUND: src/lib.rs
- FOUND: tests/fixtures/devicemgmt.wsdl
- FOUND: tests/fixtures/onvif.xsd
- FOUND: tests/fixtures/common.xsd
- FOUND: src/wsdl/mod.rs
- FOUND: src/xsd/mod.rs
- FOUND: src/wssec/mod.rs
- FOUND commit: 9d86835 (chore(01-01): add Cargo.toml with all locked dependencies)
- FOUND commit: 69ebc86 (feat(01-01): add module skeleton and ONVIF test fixtures)
