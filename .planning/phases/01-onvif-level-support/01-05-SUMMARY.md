---
phase: 01-onvif-level-support
plan: 05
subsystem: xsd
tags: [xsd, resolver, extension-chain, cycle-detection, import, schema-loader, quick-xml]

# Dependency graph
requires:
  - phase: 01-onvif-level-support/01-03
    provides: RawSchema from parse_schema() and all XsdType/ComplexContent variants

provides:
  - resolve_schema() entry point that flattens extension chains and merges imported schemas into TypeRegistry
  - SchemaLoader trait for abstracting file I/O in production vs. inline strings in tests
  - NullSchemaLoader for tests that don't need import loading
  - Cycle detection via CycleDetected error for self-referential or transitive type cycles

affects: [01-onvif-level-support/01-06, 01-onvif-level-support/01-07, wsdl-resolver]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Two-pass XSD processing: parse_schema (pass 1) -> resolve_schema (pass 2)"
    - "Bottom-up extension resolution: deepest ancestor resolved first, elements prepended"
    - "SchemaLoader trait pattern: production uses FileSchemaLoader, tests use inline MockSchemaLoader"
    - "already_loaded HashMap keyed by location string for diamond import deduplication"

key-files:
  created:
    - src/xsd/resolver.rs
  modified:
    - src/xsd/mod.rs
    - src/wssec/username_token.rs

key-decisions:
  - "resolve_schema() returns TypeRegistry with no ComplexExtension variants remaining — all resolved to Sequence/All/Choice"
  - "xs:restriction resolution uses restriction's own content model only (not base's elements)"
  - "already_loaded keyed by schema location string (not namespace) — location is the stable dedup key"
  - "BytesText::unescape() -> decode() for quick-xml 0.39 API compatibility"

patterns-established:
  - "SchemaLoader trait: fn load(&self, namespace: Option<&str>, location: &str) -> Result<String, SchemaError>"
  - "resolve_named_type() checks resolved cache first, then resolving set for cycle detection, then resolves fresh"
  - "xs:any stored as __any__ synthetic element name, detected and passed through by resolver"

requirements-completed: [XSD-03, XSD-04]

# Metrics
duration: 8min
completed: 2026-04-03
---

# Phase 1 Plan 5: XSD Resolver Summary

**resolve_schema() flattens xs:extension chains bottom-up (ancestor elements first), handles xs:restriction, loads xs:import/xs:include via SchemaLoader trait with diamond-import deduplication and cycle detection**

## Performance

- **Duration:** 8 min
- **Started:** 2026-04-03T18:19:01Z
- **Completed:** 2026-04-03T18:27:00Z
- **Tasks:** 1
- **Files modified:** 3

## Accomplishments

- Implemented `resolve_schema()` that resolves all ComplexExtension and ComplexRestriction variants into concrete Sequence/All/Choice content models with elements in ancestor-first order
- 3-level inheritance test (BaseType -> MiddleType -> LeafType) produces [id, name, value] in correct order
- SchemaLoader trait enables file I/O in production and inline XML strings in tests; NullSchemaLoader for import-free scenarios
- Diamond import deduplication: `already_loaded` HashMap keyed by location prevents D.xsd loading twice when both B.xsd and C.xsd import it
- CycleDetected returned when a type directly or transitively references itself during resolution
- xs:group ref= inlining, xs:attributeGroup ref= expansion, xs:element ref= resolution all handled

## Task Commits

Each task was committed atomically:

1. **Task 1: XSD resolver — extension chains, restriction, import/include with cycle detection** - `1b1d928` (feat)

**Plan metadata:** (pending final docs commit)

## Files Created/Modified

- `src/xsd/resolver.rs` — SchemaLoader trait, NullSchemaLoader, resolve_schema(), resolve_named_type(), resolve_complex_type(), resolve_content(), resolve_element_list(), resolve_attributes(), expand_attribute_group(), 8 test cases
- `src/xsd/mod.rs` — re-exports resolve_schema, SchemaLoader, NullSchemaLoader
- `src/wssec/username_token.rs` — fixed BytesText::unescape() -> decode() for quick-xml 0.39

## Decisions Made

- `already_loaded` keyed by schema location string rather than namespace — namespace can be absent on xs:include, and the same namespace can have multiple locations in theory
- xs:restriction uses only the restriction's own content model (not the base's) — this matches XSD spec and python-zeep behavior
- No `ResolvedSchema` wrapper struct — TypeRegistry is sufficient; no need for an intermediate type

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed BytesText::unescape() -> decode() for quick-xml 0.39**
- **Found during:** Task 1 (initial cargo test run)
- **Issue:** `src/wssec/username_token.rs` called `e.unescape()` on `BytesText<'_>`, which does not exist in quick-xml 0.39 (method renamed to `decode()`)
- **Fix:** Changed `e.unescape()` to `e.decode()` — same return type `Result<Cow<str>, EncodingError>`
- **Files modified:** src/wssec/username_token.rs
- **Verification:** `cargo check` exits 0; all xsd:: tests pass
- **Committed in:** `1b1d928` (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (Rule 1 - Bug)
**Impact on plan:** Fix was necessary to allow tests to compile. No scope creep.

## Issues Encountered

- Pre-existing compilation error in `username_token.rs` due to quick-xml 0.39 API change blocked test execution. Fixed inline as Rule 1 deviation.

## Next Phase Readiness

- TypeRegistry with fully resolved types is ready to consume in WSDL binding resolution (plan 06/07)
- SchemaLoader trait is in place for FileSchemaLoader implementation in the wsdl module
- The highest-risk component (3-level inheritance, cycle detection) is validated and green

---
*Phase: 01-onvif-level-support*
*Completed: 2026-04-03*
