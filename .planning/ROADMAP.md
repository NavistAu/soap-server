# Roadmap: soap-server

## Overview

Two phases follow the natural delivery boundary: Phase 1 delivers everything needed to unblock onvif-server — XSD/WSDL parsing, SOAP 1.2 envelope handling, document/literal dispatch, raw handler API, WS-Security UsernameToken, and axum Router integration. Phase 2 extends to full spec compliance by adding SOAP 1.1 support, SOAP 1.1 fault format, RPC/encoded dispatch, and multi-service WSDL routing.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [x] **Phase 1: ONVIF-Level Support** - Everything needed to unblock onvif-server: XSD/WSDL parsing, SOAP 1.2 pipeline, WS-Security UsernameToken, axum Router integration (completed 2026-04-05)
- [x] **Phase 2: Full Spec Compliance** - SOAP 1.1 envelope and fault support, RPC/encoded dispatch, multiple services per WSDL (completed 2026-04-05)
- [ ] **Phase 3: Audit Gap Closure** - Multi-service WSDL GET route, public API surface cleanup, stale TODO removal, documentation fixes

## Phase Details

### Phase 1: ONVIF-Level Support
**Goal**: A consumer can point the server at a real ONVIF WSDL, register raw handlers, and serve authenticated SOAP 1.2 requests over axum — everything onvif-server needs to function
**Depends on**: Nothing (first phase)
**Requirements**: XSD-01, XSD-02, XSD-03, XSD-04, XSD-05, XSD-06, XSD-07, XSD-08, XSD-09, XSD-10, XSD-11, WSDL-01, WSDL-02, WSDL-03, WSDL-04, WSDL-05, ENV-01, ENV-02, ENV-03, ENV-04, FLT-01, FLT-02, FLT-03, DSP-01, DSP-02, DSP-03, DSP-04, HDL-01, HDL-02, HDL-03, SEC-01, SEC-02, SEC-03, SEC-04, SEC-05, SEC-06, SEC-07, HTTP-01, HTTP-02, HTTP-03, HTTP-04
**Success Criteria** (what must be TRUE):
  1. A multi-file ONVIF WSDL with cross-file XSD imports loads without panicking and produces a non-empty ServiceModel with all operations and type graphs resolved
  2. A POST with a valid SOAP 1.2 envelope dispatches to the correct handler by body element QName and returns the handler's response wrapped in a SOAP 1.2 envelope
  3. A request with a valid WS-Security UsernameToken PasswordDigest is accepted and dispatched; a wrong password, malformed digest, expired timestamp, or replayed nonce is rejected with a SOAP fault before handler invocation
  4. GET `?wsdl` returns the WSDL XML with `soap:address location` rewritten to the server's actual URL
  5. The server returns an `axum::Router` that composes cleanly with other routes via `Router::merge`
**Plans**: 10 plans

Plans:
- [ ] 01-01-PLAN.md — Crate scaffold: Cargo.toml, module skeleton, ONVIF test fixtures
- [ ] 01-02-PLAN.md — Foundation types: fault.rs, handler.rs, xsd/types.rs, xsd/elements.rs, wsdl/definitions.rs
- [ ] 01-03-PLAN.md — XSD Pass 1 parser: all visit_* functions for complexType, simpleType, element, attribute, group, any
- [ ] 01-04-PLAN.md — SOAP envelope parse/serialize and WS-Security timestamp + nonce cache
- [ ] 01-05-PLAN.md — XSD Pass 2 resolver: extension chain flattening, restriction, import/include with cycle detection
- [ ] 01-06-PLAN.md — WS-Security UsernameToken validation and WSDL Pass 1 parser
- [ ] 01-07-PLAN.md — WSDL Pass 2 resolver: cross-ref wiring, import loading, schema delegation, address rewriting
- [ ] 01-08-PLAN.md — Dispatch table and XSD payload validation (DSP-01–04, XSD-11)
- [ ] 01-09-PLAN.md — ServerBuilder, SoapService, full request pipeline, axum Router integration
- [ ] 01-10-PLAN.md — ONVIF end-to-end integration tests (phase acceptance gate)

### Phase 2: Full Spec Compliance
**Goal**: The server handles SOAP 1.1 requests with correct envelope parsing and fault format, dispatches RPC/encoded bindings, and routes multi-service WSDLs — covering the full SOAP spec beyond ONVIF's subset
**Depends on**: Phase 1
**Requirements**: ENV-05, ENV-06, FLT-04, FLT-05, DSP-05, DSP-06
**Success Criteria** (what must be TRUE):
  1. A POST with `Content-Type: text/xml` and a SOAP 1.1 envelope is parsed, dispatched, and returns a SOAP 1.1 response envelope — no SOAP 1.2 namespaces leak in
  2. Errors on SOAP 1.1 requests return faults with `faultcode`, `faultstring`, `faultactor`, and `detail` elements — not SOAP 1.2 Code/Reason structure
  3. Fault codes map correctly between versions: Sender maps to Client, Receiver maps to Server, and vice versa
  4. A WSDL defining multiple services routes each service's operations to its own dispatch table without collision
  5. An RPC/encoded binding request is dispatched to the correct handler without panicking
**Plans**: 3 plans

Plans:
- [ ] 02-01-PLAN.md — SOAP 1.1 envelope unit tests + fix fault_response() Content-Type
- [ ] 02-02-PLAN.md — SOAP 1.1 fault serializer (faultcode/faultstring) + versioned fault dispatch + integration tests
- [ ] 02-03-PLAN.md — RPC dispatch QName synthesis + per-service multi-service routing

### Phase 3: Audit Gap Closure
**Goal**: Close all gaps from v1.0 milestone audit — add WSDL GET route in multi-service mode, re-export internal types for public API surface, remove stale TODO comments, fix documentation gaps
**Depends on**: Phase 2
**Requirements**: WSDL-04 (multi-service extension), DSP-06 (multi-service completeness)
**Gap Closure:** Closes gaps from v1.0 audit
**Success Criteria** (what must be TRUE):
  1. GET /soap/a?wsdl returns WSDL XML in multi-service mode (not 405)
  2. RotatingNonceCache, DispatchTable, build_dispatch_table, validate_username_token are accessible from lib.rs public API
  3. No stale TODO comments remain in src/fault.rs or src/envelope.rs
  4. REQUIREMENTS.md ENV-05/ENV-06 checkboxes are checked and traceability shows Complete
  5. 02-02-SUMMARY.md frontmatter includes FLT-04, FLT-05 in requirements-completed
**Plans**: 2 plans

Plans:
- [ ] 03-01-PLAN.md — WSDL GET in multi-service mode + public API re-exports + stale TODO removal
- [ ] 03-02-PLAN.md — REQUIREMENTS.md checkbox fixes + 02-02-SUMMARY.md frontmatter backfill

## Progress

**Execution Order:**
Phases execute in numeric order: 1 → 2

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. ONVIF-Level Support | 10/10 | Complete    | 2026-04-05 |
| 2. Full Spec Compliance | 3/3 | Complete   | 2026-04-05 |
| 3. Audit Gap Closure | 0/2 | Pending    | — |
