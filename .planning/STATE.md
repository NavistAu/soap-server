---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: milestone
status: Gap closure phase created from v1.0 audit
stopped_at: Gap closure phase 4 created from v1.0 re-audit
last_updated: "2026-04-05T06:50:00.000Z"
last_activity: 2026-04-05 — v1.0 re-audit complete, phase 4 created for WSDL address fix
progress:
  total_phases: 4
  completed_phases: 3
  total_plans: 15
  completed_plans: 15
  percent: 94
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-04-03)

**Core value:** Given a WSDL file and handler functions, serve a fully spec-compliant SOAP endpoint with correct envelope parsing, dispatch, fault generation, and WSDL serving.
**Current focus:** Phase 3 — Audit Gap Closure

## Current Position

Phase: 3 of 3 (Audit Gap Closure) — PENDING
Plan: 0 of 0 — needs planning
Status: Gap closure phase created from v1.0 audit
Last activity: 2026-04-05 — v1.0 audit complete, gap closure phase 3 created

Progress: [████████░░] 87%

## Performance Metrics

**Velocity:**
- Total plans completed: 0
- Average duration: —
- Total execution time: 0 hours

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| - | - | - | - |

**Recent Trend:**
- Last 5 plans: —
- Trend: —

*Updated after each plan completion*
| Phase 01-onvif-level-support P01-01 | 3 | 2 tasks | 26 files |
| Phase 01-onvif-level-support P02 | 5min | 2 tasks | 10 files |
| Phase 01-onvif-level-support P04 | 3min | 2 tasks | 4 files |
| Phase 01-onvif-level-support P03 | 8min | 1 tasks | 2 files |
| Phase 01-onvif-level-support P05 | 8min | 1 tasks | 3 files |
| Phase 01-onvif-level-support P06 | 10min | 2 tasks | 4 files |
| Phase 01-onvif-level-support P07 | 12min | 1 tasks | 4 files |
| Phase 01-onvif-level-support P08 | 5min | 1 tasks | 1 files |
| Phase 01-onvif-level-support P09 | 7min | 1 tasks | 4 files |
| Phase 01-onvif-level-support PP10 | 20min | 1 tasks | 6 files |
| Phase 01-onvif-level-support P10 | 20min | 2 tasks | 6 files |
| Phase 02-full-spec-compliance P03 | 10min | 2 tasks | 3 files |
| Phase 02-full-spec-compliance P02 | 3min | 2 tasks | 3 files |
| Phase 03-audit-gap-closure P02 | 1min | 2 tasks | 2 files |
| Phase 03-audit-gap-closure P03-01 | 8min | 2 tasks | 4 files |

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- [Init]: roxmltree for WSDL/XSD startup parse; quick-xml for per-request streaming
- [Init]: Two-pass parse pattern mandatory — single-pass cannot handle forward references in ONVIF WSDLs
- [Init]: Raw handler trait is primary API — typed handler deferred to v2
- [2026-04-03]: Restructured from 4 phases to 2 — Phase 1 is everything needed to unblock onvif-server (XSD/WSDL parsing + full SOAP 1.2 pipeline including WS-Security); Phase 2 is SOAP 1.1 and broader spec compliance
- [Phase 01-onvif-level-support]: axum-test uses calendar versioning (20.x not 0.16); .tool-versions pins Rust 1.85.1 for edition2024; real ONVIF fixtures from onvif.org bundled as canonical test basis
- [Phase 01-onvif-level-support]: Rust 1.88.0 required — axum-test 20.0.0 transitive deps (time/icu crates) need rustc 1.88.0 minimum
- [Phase 01-onvif-level-support]: Box<XsdType> for XsdElement.inline_type to break ComplexType -> ComplexContent -> Vec<XsdElement> recursive cycle
- [Phase 01-onvif-level-support]: Namespace re-emission on body fragment uses xmlns:* attribute inspection on Envelope element (not NsReader.resolver().bindings()) — quick-xml 0.39 does not surface a bindings iterator at the Start event level
- [Phase 01-onvif-level-support]: force_rotate() test helper added to RotatingNonceCache under #[cfg(test)] to enable deterministic bucket rotation testing without sleeps
- [Phase 01-onvif-level-support]: xs:any stored as synthetic XsdElement named __any__ in compositor Vec — keeps sequence/all/choice homogeneous; pass-2 resolver detects by name
- [Phase 01-onvif-level-support]: Extension attributes not merged into ComplexType.attributes in pass 1 — cross-schema attribute expansion is a pass-2 concern
- [Phase 01-onvif-level-support]: resolve_schema() returns TypeRegistry with no ComplexExtension variants remaining — all resolved to Sequence/All/Choice
- [Phase 01-onvif-level-support]: already_loaded keyed by schema location string for diamond import deduplication; BytesText::unescape() renamed to decode() in quick-xml 0.39
- [Phase 01-onvif-level-support]: Plan test vector for PasswordDigest was invalid base64; replaced with self-consistent verified vector (nonce=AAECAwQFBgcICQoLDA0ODw==) verified with Python hashlib
- [Phase 01-onvif-level-support]: quick-xml namespace resolution uses running HashMap accumulator — handles xmlns declared on ancestor elements, not just current element
- [Phase 01-onvif-level-support]: accumulated_types HashMap threaded through resolve_wsdl_inner recursion for correct cross-WSDL type deduplication
- [Phase 01-onvif-level-support]: serialize_node() must emit xs:-prefixed element names via find_prefix_for_ns() — bare local names cause inline schema strings to inherit WSDL default namespace
- [Phase 01-onvif-level-support]: DispatchEntry.auth_required set at build time from auth_bypass HashSet — avoids per-request set lookup; security interceptor reads a bool
- [Phase 01-onvif-level-support]: validate_request skips validation silently when input_type is None or qname not in registry — unknown type is not an error (forward-compat with partial WSDLs)
- [Phase 01-onvif-level-support]: TestServer::new().bytes().content_type() — axum-test .text() overrides content-type with text/plain; must use .bytes() to preserve application/soap+xml
- [Phase 01-onvif-level-support]: ServerBuilder::default_handler() required for large multi-operation WSDLs — build() fails with UnregisteredOperation for every unhandled op without it
- [Phase 01-onvif-level-support]: XSD resolver tolerant unknown refs — external schemas (wsn/b-2, xop/include) return Empty ComplexType; unknown type is not an error
- [Phase 01-onvif-level-support]: Header namespace re-emission fix in collect_header_children() — envelope xmlns:* bindings must be re-emitted on extracted header child fragments for wsse: prefix resolution
- [Phase 01-onvif-level-support]: FixtureLoader maps by basename — real ONVIF relative paths resolved to tests/fixtures/ files by extracting last path component
- [Phase 02-full-spec-compliance]: RPC QName synthesized as QName{ns=soap:body.namespace or targetNamespace, local=op_name} at build time in collect_ops_for_service()
- [Phase 02-full-spec-compliance]: SoapServiceRoute thin struct carries per-service DispatchTable as axum State for multi-service routing; service_tables empty in single-service mode preserving full backward compatibility
- [Phase 02-full-spec-compliance]: to_xml_bytes_v11() is private; external callers use to_xml_bytes_versioned(&SoapVersion) for version dispatch
- [Phase 02-full-spec-compliance]: DataEncodingUnknown maps to SOAP-ENV:Server in SOAP 1.1 (no equivalent), matching Apache CXF behavior
- [Phase 02-full-spec-compliance]: axum-test TestResponse uses as_bytes() not bytes() — confirmed during Task 2 green phase
- [Phase 03-audit-gap-closure]: ENV-05 and ENV-06 were implemented in Phase 2 (02-01) but left unchecked in REQUIREMENTS.md — marked complete in audit gap closure
- [Phase 03-audit-gap-closure]: Multi-service WSDL GET uses separate router.route(path, get(wsdl_get_handler).with_state(state.clone())) chained after POST route — axum allows multiple route() calls on the same path with different states
- [Phase 03-audit-gap-closure]: dispatch module promoted from pub(crate) to pub to enable DispatchTable re-export from crate root

### Pending Todos

None yet.

### Blockers/Concerns

None — Phase 1 complete. Both previously logged blockers were resolved during execution:
- [x] XSD extension/restriction resolution — two-pass resolver with ComplexExtension flattening implemented in plan 07
- [x] Namespace inheritance loss — collect_header_children() re-emits envelope xmlns:* bindings on extracted fragments (plan 10)

## Session Continuity

Last session: 2026-04-05T06:35:57.005Z
Stopped at: Completed 03-audit-gap-closure 03-01-PLAN.md
Resume file: None
