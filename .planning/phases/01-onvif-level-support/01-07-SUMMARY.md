---
phase: 01-onvif-level-support
plan: "07"
subsystem: wsdl
tags: [wsdl, xsd, roxmltree, quick-xml, resolver, import, type-registry, soap12]

requires:
  - phase: 01-onvif-level-support/01-06
    provides: parse_wsdl(), WsdlDefinition, WsdlError — Pass 1 parser consumed by resolver
  - phase: 01-onvif-level-support/01-05
    provides: resolve_schema(), TypeRegistry, SchemaLoader — XSD pass-2 resolver delegated to from WSDL resolver

provides:
  - resolve_wsdl() — Pass 2 WSDL resolution: recursive import loading, inline schema delegation, TypeRegistry construction
  - WsdlLoader trait — abstracts WSDL file loading for recursive import resolution
  - ResolvedWsdl — output type bundling WsdlDefinition + TypeRegistry + raw_bytes
  - rewrite_wsdl_address() — streaming soap:address location rewrite for GET ?wsdl serving
  - TypeRegistry IntoIterator — allows merging partial registries across recursive WSDL import resolution

affects:
  - 01-08 model.rs (consumes ResolvedWsdl to build ServiceModel)
  - 01-09 router (uses rewrite_wsdl_address for WSDL serving endpoint)

tech-stack:
  added: []
  patterns:
    - visited HashSet passed through recursive import resolution for diamond-import deduplication (zeep pattern)
    - accumulated_types HashMap threaded through recursion to collect XSD types from all imported WSDLs
    - WsdlSchemaLoaderAdapter bridges WsdlLoader to SchemaLoader interface for inline XSD resolution
    - serialize_node() must emit qualified element names (xs:complexType) not bare local names — WSDL default namespace contaminates inline schema strings if not corrected
    - quick-xml streaming rewrite for address rewriting avoids full parse cost

key-files:
  created:
    - src/wsdl/resolver.rs
  modified:
    - src/wsdl/mod.rs
    - src/wsdl/parser.rs
    - src/xsd/types.rs

key-decisions:
  - "accumulated_types HashMap threaded through resolve_wsdl_inner recursion (not returned with ResolvedWsdl) ensures types from transitively imported WSDLs accumulate correctly in a single shared map"
  - "serialize_node() in parser.rs fixed to emit xs:-prefixed element names using find_prefix_for_ns() — bare local names caused inline schema strings to inherit WSDL default namespace, making them unparseable as XSD"
  - "TypeRegistry gained IntoIterator (consuming) to enable merging partial registries; iterator over HashMap<QName, XsdType>.into_iter()"

requirements-completed: [WSDL-02, WSDL-03, WSDL-04, WSDL-05]

duration: 12min
completed: 2026-04-03
---

# Phase 01 Plan 07: WSDL Pass 2 Resolver Summary

**WSDL resolver wiring cross-references with recursive import loading, inline XSD schema delegation to TypeRegistry, and streaming WSDL address rewriting for GET ?wsdl serving**

## Performance

- **Duration:** ~12 min
- **Started:** 2026-04-03T18:31:18Z
- **Completed:** 2026-04-03T18:43:00Z
- **Tasks:** 1
- **Files modified:** 4

## Accomplishments
- `resolve_wsdl()` implements the full WSDL Pass 2 pipeline: parse → merge imports → collect inline schemas → resolve XSD types → return ResolvedWsdl
- Diamond import deduplication via visited HashSet matches the zeep reference implementation (A imports B and C; B and C both import D → D loaded once)
- Inline `xs:schema` nodes from `wsdl:types` are parsed via `xsd::parse_schema` and resolved via `xsd::resolve_schema`, populating a TypeRegistry
- `rewrite_wsdl_address()` uses quick-xml event streaming to replace `soap:address`/`soap12:address` location attribute without full document parse
- Fixed a latent bug in the WSDL parser's `serialize_node()`: elements were emitted with bare local names, causing the WSDL default namespace to contaminate inline schema strings

## Task Commits

1. **Task 1: WSDL resolver — cross-reference wiring, import loading, schema delegation** - `bbd05ce` (feat)

**Plan metadata:** (docs commit follows)

## Files Created/Modified
- `src/wsdl/resolver.rs` — resolve_wsdl(), WsdlLoader trait, ResolvedWsdl, rewrite_wsdl_address(), 10 unit tests
- `src/wsdl/mod.rs` — exports resolve_wsdl, ResolvedWsdl, WsdlLoader, rewrite_wsdl_address
- `src/wsdl/parser.rs` — fixed serialize_node() to emit qualified element names; added find_prefix_for_ns() helper
- `src/xsd/types.rs` — added IntoIterator for TypeRegistry to enable merging partial registries

## Decisions Made
- Threaded `accumulated_types: &mut HashMap<QName, XsdType>` through the recursive `resolve_wsdl_inner` signature rather than merging registries after the fact — ensures all types from all imports share a single accumulated map with correct deduplication semantics
- TypeRegistry snapshot is built from the accumulated map at each recursion level (via `.iter()` + clone), so each `ResolvedWsdl` carries a complete snapshot of everything loaded so far
- `serialize_node()` needed `find_prefix_for_ns()` helper to look up the correct prefix for an element's namespace URI in the in-scope namespaces — without this, `xs:complexType` was serialized as `complexType` in the WSDL default namespace

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed serialize_node() emitting unqualified element names in inline schema serialization**
- **Found during:** Task 1 (WSDL resolver — inline schema resolution)
- **Issue:** `node_to_string()` in parser.rs used `node.tag_name().name()` (local name only), emitting `<complexType>` instead of `<xs:complexType>`. When the serialized schema string is re-parsed standalone, the WSDL default namespace `xmlns="http://schemas.xmlsoap.org/wsdl/"` (inherited from the WSDL root) applies to all unqualified elements, making XSD elements appear to be WSDL elements. `xsd::parse_schema` found 0 types because it filters by XSD namespace.
- **Fix:** Added `find_prefix_for_ns()` helper that scans in-scope namespaces to find the prefix bound to an element's namespace URI. `serialize_node()` now emits `xs:complexType`, `xs:sequence`, etc. The fix is minimal and correct: if no prefix is found, the bare local name is used (safe fallback).
- **Files modified:** src/wsdl/parser.rs
- **Verification:** `standalone_wsdl_resolves_inline_schema` test passes; TypeRegistry has 1 type after fix (was 0 before)
- **Committed in:** bbd05ce (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1x Rule 1 - Bug)
**Impact on plan:** Fix was necessary for correctness — without it all inline schema types would silently disappear. No scope creep.

## Issues Encountered
- `TypeRegistry` had no `IntoIterator` implementation, blocking the pattern of merging partial registries via `for (qname, xsd_type) in partial`. Added `impl IntoIterator for TypeRegistry` (consuming iterator over internal HashMap). This is a small additive change to xsd/types.rs.

## Next Phase Readiness
- `ResolvedWsdl` is the complete output ready for consumption by model.rs (plan 08)
- `WsdlLoader` trait allows the production file system loader to be implemented separately
- `rewrite_wsdl_address()` is ready for use in the WSDL serving GET ?wsdl route
- All 38 wsdl:: tests pass, all prior test suites unaffected

## Self-Check: PASSED

- src/wsdl/resolver.rs: FOUND
- src/wsdl/mod.rs: FOUND (exports resolve_wsdl, ResolvedWsdl, WsdlLoader, rewrite_wsdl_address)
- Commit bbd05ce (Task 1): FOUND
- cargo check: exits 0
- 38/38 wsdl:: tests pass

---
*Phase: 01-onvif-level-support*
*Completed: 2026-04-03*
