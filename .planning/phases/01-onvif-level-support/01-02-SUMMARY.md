---
phase: 01-onvif-level-support
plan: "02"
subsystem: core-types
tags: [rust, soap, fault, handler, xsd, wsdl, async-trait, tdd]

requires:
  - 01-01
provides:
  - src/fault.rs with SoapFault, FaultCode, SOAP 1.2 XML serialization
  - src/handler.rs with SoapHandler trait, FnHandler closure wrapper
  - src/qname.rs with QName (namespace + local_name, Hash, Eq, Display)
  - src/xsd/types.rs with ComplexType, SimpleType, TypeRegistry and all sub-types
  - src/xsd/elements.rs with XsdElement, XsdAttribute, MaxOccurs, AttributeGroup, Group, AnyElement, AnyAttribute
  - src/wsdl/definitions.rs with WsdlDefinition and all WSDL 1.1 structs
affects: [03, 04, 05, 06, 07, 08, 09, 10]

tech-stack:
  added:
    - async-trait 0.1 (async methods in traits for SoapHandler)
  patterns:
    - TDD: fault.rs tests written alongside implementation, 14 fault + 4 handler tests all green
    - String formatting for fault XML (small fixed-structure, no xml writer needed)
    - Box<XsdType> for inline_type in XsdElement to break recursive type cycle

key-files:
  created:
    - src/qname.rs
  modified:
    - src/fault.rs
    - src/handler.rs
    - src/xsd/types.rs
    - src/xsd/elements.rs
    - src/xsd/mod.rs
    - src/wsdl/definitions.rs
    - src/wsdl/mod.rs
    - src/lib.rs
    - Cargo.toml
    - .tool-versions

key-decisions:
  - "Rust bumped to 1.88.0 in .tool-versions — axum-test 20.0.0 transitive deps (time 0.3.47 via icu_* crates) require rustc 1.88.0 minimum; 1.85.1 was no longer sufficient"
  - "Box<crate::xsd::types::XsdType> for inline_type in XsdElement — XsdType contains ComplexType which contains Vec<XsdElement>, so the box is required to avoid an infinite-size recursive type"
  - "TypesSection.schemas stores raw XML strings — full schema parsing happens in the XSD parser plan (03), not here; this avoids a circular dependency between WSDL and XSD modules at the types layer"

duration: 5min
completed: "2026-04-03"
---

# Phase 1 Plan 2: Core Types and Public API Contracts Summary

**SoapFault with SOAP 1.2 XML serialization, SoapHandler async trait, QName, and all XSD/WSDL data type structs defined — 47 tests passing, cargo check clean**

## Performance

- **Duration:** ~5 min
- **Started:** 2026-04-03T18:04:56Z
- **Completed:** 2026-04-03T18:09:16Z
- **Tasks:** 2
- **Files modified:** 10

## Accomplishments

- SoapFault with all 5 SOAP 1.2 FaultCodes serializing to correct envelope XML per W3C spec Section 7.4.2
- SoapHandler async trait using async-trait with FnHandler closure wrapper usable as a trait object
- QName struct with optional namespace, Hash+Eq for use as HashMap key, Display in Clark notation
- Full XSD type hierarchy: XsdType, ComplexType, ComplexContent (7 variants), SimpleType, Restriction (13 facets), WhitespaceHandling, ListDef, UnionDef, SimpleContentDef, TypeRegistry
- Full XSD element hierarchy: XsdElement (with default min_occurs=1), MaxOccurs, XsdAttribute, AttributeUse, AttributeGroup, Group, GroupContent, AnyElement, AnyNamespace, ProcessContents, AnyAttribute
- Full WSDL 1.1 struct set: WsdlDefinition, TypesSection, Message, MessagePart, PortType, Operation, OperationStyle, OperationMessage, OperationFault, Binding, SoapBinding, BindingStyle, SoapVersion, BindingOperation, BindingMessage, SoapBody, UseStyle, SoapHeader, Service, Port, WsdlImport
- 47 unit tests all passing

## Task Commits

1. **Task 1: fault.rs and handler.rs** — `022f1bf` (feat)
2. **Task 2: XSD and WSDL data type definitions** — `0fe3708` (feat)

## Files Created/Modified

- `src/fault.rs` — SoapFault, FaultCode, to_xml_bytes(), 14 tests
- `src/handler.rs` — SoapHandler trait, FnHandler, 4 tests
- `src/qname.rs` — QName with namespace, local_name, Display, 5 tests
- `src/xsd/types.rs` — XsdType, ComplexType, ComplexContent, SimpleType, Restriction, TypeRegistry, 6 tests
- `src/xsd/elements.rs` — XsdElement, MaxOccurs, XsdAttribute, AttributeUse, AttributeGroup, Group, AnyElement, 10 tests
- `src/xsd/mod.rs` — updated with pub re-exports
- `src/wsdl/definitions.rs` — WsdlDefinition and all WSDL structs, 9 tests
- `src/wsdl/mod.rs` — updated with pub re-exports
- `src/lib.rs` — added pub mod qname
- `Cargo.toml` — added async-trait = "0.1"
- `.tool-versions` — bumped Rust from 1.85.1 to 1.88.0

## Decisions Made

- Rust 1.88.0 required — axum-test 20.0.0 now pulls in `time 0.3.47` via ICU crates, which requires rustc 1.88.0. Previous pin of 1.85.1 broke cargo resolve. 1.88.0 is already installed via mise.
- `Box<crate::xsd::types::XsdType>` for `XsdElement.inline_type` — XsdType → ComplexType → ComplexContent → Vec<XsdElement> creates an infinite-size cycle without boxing.
- `TypesSection.schemas: Vec<String>` stores raw XML strings — full XSD parse happens in plan 03. Avoids circular dependency at the types layer.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocker] Rust 1.85.1 incompatible with axum-test 20.0.0 transitive dependencies**
- **Found during:** Task 1 (first cargo test run)
- **Issue:** axum-test 20.0.0 now transitively requires `time 0.3.47` and `icu_*` crates which require rustc 1.86 and 1.88 respectively. Cargo refused to build.
- **Fix:** Updated `.tool-versions` from `rust 1.85.1` to `rust 1.88.0` (already installed via mise)
- **Files modified:** `.tool-versions`
- **Commit:** 022f1bf (Task 1 commit)

None beyond the above auto-fixed deviation. Plan executed as specified.

## Self-Check: PASSED

- FOUND: src/fault.rs (SoapFault, FaultCode)
- FOUND: src/handler.rs (SoapHandler trait)
- FOUND: src/qname.rs (QName)
- FOUND: src/xsd/types.rs (ComplexType, TypeRegistry)
- FOUND: src/xsd/elements.rs (XsdElement, MaxOccurs)
- FOUND: src/wsdl/definitions.rs (WsdlDefinition)
- FOUND commit: 022f1bf
- FOUND commit: 0fe3708
- cargo check: PASSED
- 47 tests: all PASSED
