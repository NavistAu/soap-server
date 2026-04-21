# soap-server

## What This Is

A general-purpose, spec-compliant SOAP server crate for Rust. Provides WSDL-driven dispatch, SOAP 1.1/1.2 envelope handling, XSD schema parsing, WS-Security UsernameToken support, and multi-service routing — all behind an ergonomic `ServerBuilder` API that returns a composable `axum::Router`.

## Core Value

Given a WSDL file and handler functions, the crate serves a fully spec-compliant SOAP endpoint — correct envelope parsing, dispatch, fault generation, and WSDL serving — so consumers don't have to implement any SOAP plumbing themselves.

## Requirements

### Validated

- ✓ WSDL 1.1 parser — full spec, two-pass (parse + resolve), diamond import dedup, cycle detection — v1.0
- ✓ XSD schema parser — full spec (complexType, simpleType, sequence, choice, all, extension, restriction, imports, groups, any) — v1.0
- ✓ SOAP 1.2 envelope parsing and serialization — v1.0
- ✓ SOAP 1.1 envelope parsing and serialization — v1.0
- ✓ Document/literal binding dispatch (body element QName -> handler, O(1)) — v1.0
- ✓ RPC/encoded binding dispatch (synthesized QName from soap:body namespace + op name) — v1.0
- ✓ WS-Security UsernameToken (PasswordDigest + PasswordText + timestamp + nonce replay) — v1.0
- ✓ Auth bypass for specific operations (e.g., GetSystemDateAndTime) — v1.0
- ✓ WSDL serving on GET ?wsdl with per-service address rewriting — v1.0
- ✓ SOAP 1.2 fault generation (Code/Reason/Detail) — v1.0
- ✓ SOAP 1.1 fault generation (faultcode/faultstring with version-aware code mapping) — v1.0
- ✓ axum Router integration (composable with other routes via Router::merge) — v1.0
- ✓ Raw handler trait (receives XML bytes, returns XML bytes or SoapFault) — v1.0
- ✓ Multiple services per WSDL (per-service dispatch tables, isolated routing) — v1.0
- ✓ XSD payload validation before handler invocation — v1.0
- ✓ ServerBuilder API (from_wsdl → handler → auth → build → into_router) — v1.0

### Active

- [ ] MTOM/XOP support — multipart MIME parsing, XOP Include element replacement
- [ ] Typed handler API — WSDL-derived type deserialization/serialization (research needed)
- [ ] WS-Addressing support — enterprise routing headers

### Out of Scope

- SOAP client functionality — server-only crate
- ONVIF-specific logic — lives in onvif-server crate
- Code generation from WSDL (proc-macro / build.rs) — runtime dispatch covers all spec functionality
- WSDL generation from Rust types — SOAP is contract-first, WSDL is the authority
- WS-Security beyond UsernameToken (X.509, SAML, Kerberos) — deferred to v2
- Built-in TLS/HTTPS — transport-layer concern, handled by axum-server or reverse proxy
- Offline mode / session state — SOAP is stateless by design

## Context

General-purpose SOAP server foundation crate. Dependency chain: `soap-server` <- `onvif-server`. Publishes to crates.io under NavistAu ownership.

**Shipped v1.0** with 7,825 LOC Rust (src/) + 1,314 LOC tests across 22 files. 205 tests (186 unit + 12 integration + 7 ONVIF). Built in 3 days (2026-04-03 to 2026-04-05), 71 commits across 4 phases (16 plans).

Key dependencies: roxmltree 0.21 (WSDL/XSD DOM parsing at startup), quick-xml 0.39 (per-request streaming), axum 0.8 + tokio (HTTP/async), sha1 + base64 + chrono (WS-Security).

License: MIT OR Apache-2.0 (dual, standard Rust convention).

Ported from python-zeep (MIT) for WSDL/XSD parsing logic. Supplementary references: node-soap (dispatch pattern), rpos (WS-Security digest).

## Constraints

- **Spec compliance**: Must follow WSDL 1.1 and SOAP 1.1/1.2 specifications — not a partial or "good enough" implementation
- **Performance**: Per-request path uses quick-xml streaming, not DOM — WSDL/XSD parsing via roxmltree is startup-only
- **Ecosystem fit**: Must compose cleanly as an axum Router so consumers can add their own routes alongside SOAP endpoints
- **First consumer**: onvif-server — now unblocked by v1.0

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| roxmltree for WSDL/XSD, quick-xml for runtime | DOM is fine at startup; streaming needed per-request | ✓ Good — clean separation, no per-request allocations from DOM |
| Port from python-zeep | Best-structured open-source WSDL/XSD implementation, MIT licensed | ✓ Good — two-pass pattern translated cleanly to Rust |
| axum as HTTP framework | Dominant Rust web framework, composes via Router | ✓ Good — Router::merge works for multi-service and composition |
| Raw handler as primary API | Typed handler needs research; raw unblocks consumers immediately | ✓ Good — unblocked onvif-server; typed handler deferred to v2 |
| Two-pass parse pattern (parse + resolve) | Handles forward references cleanly, proven in zeep | ✓ Good — resolves diamond imports, cycle detection, cross-file refs |
| NsReader streaming for envelope parse | Namespace re-emission on body fragment extraction | ✓ Good — ancestor xmlns:* bindings re-emitted correctly |
| RotatingNonceCache two-bucket design | Time-windowed replay detection without unbounded growth | ✓ Good — deterministic bucket rotation, no cleanup threads |
| MatchedPath for multi-service WSDL GET | Per-service soap:address rewrite without custom state | ✓ Good — zero-overhead via axum extractor |

---
*Last updated: 2026-04-05 after v1.0 milestone*
