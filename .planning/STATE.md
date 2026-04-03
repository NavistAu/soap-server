# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-04-03)

**Core value:** Given a WSDL file and handler functions, serve a fully spec-compliant SOAP endpoint with correct envelope parsing, dispatch, fault generation, and WSDL serving.
**Current focus:** Phase 1 — ONVIF-Level Support

## Current Position

Phase: 1 of 2 (ONVIF-Level Support)
Plan: 0 of TBD in current phase
Status: Ready to plan
Last activity: 2026-04-03 — Roadmap restructured from 4 phases to 2 phases

Progress: [░░░░░░░░░░] 0%

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

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- [Init]: roxmltree for WSDL/XSD startup parse; quick-xml for per-request streaming
- [Init]: Two-pass parse pattern mandatory — single-pass cannot handle forward references in ONVIF WSDLs
- [Init]: Raw handler trait is primary API — typed handler deferred to v2
- [2026-04-03]: Restructured from 4 phases to 2 — Phase 1 is everything needed to unblock onvif-server (XSD/WSDL parsing + full SOAP 1.2 pipeline including WS-Security); Phase 2 is SOAP 1.1 and broader spec compliance

### Pending Todos

None yet.

### Blockers/Concerns

- [Phase 1]: XSD extension/restriction resolution is the highest-risk piece — must write 3-level inheritance fixtures before implementing. Reference python-zeep `xsd/elements/complex.py`.
- [Phase 1]: Namespace inheritance loss when extracting body bytes — re-emit all in-scope namespace declarations on the fragment root, or pass a `(bytes, namespace_map)` tuple. API must be decided before HDL-01 is finalized.

## Session Continuity

Last session: 2026-04-03
Stopped at: Roadmap restructured to 2 phases — ready to run plan-phase 1
Resume file: None
