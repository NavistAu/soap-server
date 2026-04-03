---
phase: 01-onvif-level-support
plan: "03"
subsystem: xsd
tags: [rust, soap, xsd, roxmltree, parser, qname, tdd]

requires:
  - phase: 01-02
    provides: XsdType, ComplexType, SimpleType, XsdElement, XsdAttribute, MaxOccurs, QName, AttributeGroup, Group, AnyElement, AnyAttribute — all data types consumed by the parser

provides:
  - src/xsd/parser.rs with parse_schema() and all visit_* functions producing RawSchema
  - RawSchema struct with types, elements, attribute_groups, groups, imports, includes
  - SchemaImport / SchemaInclude structs for resolver cycle detection
  - SchemaError with MalformedXml, UnknownRef, CycleDetected variants (thiserror)
  - QName namespace resolution from prefixed attribute values via roxmltree lookup_namespace_uri

affects: [04, 05, 06, 07, 08, 09, 10]

tech-stack:
  added: []
  patterns:
    - TDD: 22 unit tests written covering all XSD constructs (all green)
    - Pure functions: roxmltree::Node in, Raw struct out — no global state
    - QName resolution at parse time using node.lookup_namespace_uri for prefix-to-URI mapping
    - r###"..."### raw strings when XML attributes contain ## (e.g. ##any for xs:any namespace)

key-files:
  created: []
  modified:
    - src/xsd/parser.rs
    - src/xsd/mod.rs

key-decisions:
  - "xs:any stored as synthetic XsdElement named __any__ inside sequence — pass 2 resolver can detect by name; full AnyElement struct available via visit_any() for callers that need it directly"
  - "visit_restriction takes a context Node parameter (the xs:simpleType parent) reserved for future namespace lookups within the restriction body"
  - "visit_extension attributes are parsed but not yet threaded into ComplexType.attributes — extension attribute expansion is a pass-2 concern"

patterns-established:
  - "Pass 1 is purely structural: parse XML → fill Raw structs; all QName resolution from prefixes to URIs happens here; cross-schema type resolution happens in pass 2"
  - "qualify_name() uses targetNamespace for top-level named types; nested anonymous types inherit context from their parent"

requirements-completed: [XSD-01, XSD-02, XSD-03, XSD-04, XSD-05, XSD-06, XSD-07, XSD-08, XSD-09, XSD-10]

duration: 8min
completed: "2026-04-03"
---

# Phase 1 Plan 3: XSD Pass 1 Parser Summary

**roxmltree DOM traversal producing RawSchema with all visit_* functions covering xs:complexType, simpleType, sequence/all/choice, extension/restriction, attribute/attributeGroup, group, any/anyAttribute, list, union, import/include — 22 unit tests passing**

## Performance

- **Duration:** ~8 min
- **Started:** 2026-04-03T18:11:39Z
- **Completed:** 2026-04-03T18:19:00Z
- **Tasks:** 1
- **Files modified:** 2

## Accomplishments

- `parse_schema()` entry point traverses `<xs:schema>` node and populates `RawSchema` with all top-level declarations
- 15 public `visit_*` functions covering every XSD construct in requirements XSD-01 through XSD-10
- QName prefix resolution at parse time using `node.lookup_namespace_uri()` — prefixed refs like `tns:Foo` become `QName { namespace: Some("urn:t"), local_name: "Foo" }`
- `SchemaImport` / `SchemaInclude` structs recorded for pass-2 resolver to load transitive schemas
- `SchemaError` enum (thiserror) with `MalformedXml`, `UnknownRef`, `CycleDetected` variants
- 22 inline unit tests covering all behaviors from the plan's `<behavior>` spec — all passing
- Total test suite: 91 tests passing, `cargo check` clean

## Task Commits

1. **Task 1: XSD Pass 1 parser — all visit functions** — `a467203` (feat)

**Plan metadata:** (docs commit follows)

## Files Created/Modified

- `src/xsd/parser.rs` — `parse_schema()`, all `visit_*` functions, `RawSchema`, `SchemaImport`, `SchemaInclude`, `SchemaError`, 22 tests (1,096 lines)
- `src/xsd/mod.rs` — added `pub use parser::{parse_schema, RawSchema, SchemaImport, SchemaInclude, SchemaError}`

## Decisions Made

- `xs:any` stored as synthetic `XsdElement` named `__any__` inside the compositor's element list — keeps the sequence/all/choice Vec homogeneous; pass-2 resolver detects by name convention. `visit_any()` is still public for direct callers.
- `visit_restriction` takes a `context` node parameter (the `xs:simpleType` parent) reserved for future namespace lookups in the restriction body — currently unused but avoids a breaking API change later.
- Extension attributes (inside `xs:extension`) are not yet merged into `ComplexType.attributes` — that merging is a pass-2 concern after cross-schema type resolution.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Raw string delimiter collision with `##any`**
- **Found during:** Task 1 (compilation of test for xs:any)
- **Issue:** Test XML containing `namespace="##any"` inside `r#"..."#` raw string caused Rust to parse `"##` as the end of the raw string literal, producing compile errors
- **Fix:** Changed the affected test to use `r###"..."###` (three hash delimiters) so `##` inside the string is not mistaken for a closing delimiter
- **Files modified:** `src/xsd/parser.rs`
- **Verification:** `cargo test xsd::parser::` compiled and passed all 22 tests
- **Committed in:** a467203 (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (Rule 1 — Rust raw string literal syntax quirk)
**Impact on plan:** Minimal — single-line fix to raw string delimiter. No design changes.

## Issues Encountered

None beyond the raw string delimiter fix above.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- `parse_schema()` is ready for plan 04 (WSDL parser) to extract inline `<xs:schema>` nodes from WSDL and pass them to `parse_schema()`
- `RawSchema` is ready for plan 05 (XSD resolver pass 2) to resolve type references and build the final `TypeRegistry`
- Highest-risk remaining work: XSD extension/restriction resolution (plan 05) — noted blocker in STATE.md

---
*Phase: 01-onvif-level-support*
*Completed: 2026-04-03*

## Self-Check: PASSED

- FOUND: src/xsd/parser.rs (parse_schema, all visit_* functions)
- FOUND: src/xsd/mod.rs (pub use parser exports)
- FOUND: .planning/phases/01-onvif-level-support/01-03-SUMMARY.md
- FOUND commit: a467203 (feat(01-03): XSD Pass 1 parser)
- cargo check: PASSED
- 22 xsd::parser:: tests: all PASSED
- 91 total tests: all PASSED
