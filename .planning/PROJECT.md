# soap-server

## What This Is

A general-purpose, spec-compliant SOAP server crate for Rust, published to crates.io. Fills a gap in the Rust ecosystem where no production SOAP server library exists. Provides WSDL-driven dispatch, SOAP 1.1/1.2 envelope handling, XSD schema parsing, and WS-Security support.

## Core Value

Given a WSDL file and handler functions, the crate serves a fully spec-compliant SOAP endpoint — correct envelope parsing, dispatch, fault generation, and WSDL serving — so consumers don't have to implement any SOAP plumbing themselves.

## Requirements

### Validated

(None yet — ship to validate)

### Active

- [ ] WSDL 1.1 parser — full spec, two-pass (parse + resolve)
- [ ] XSD schema parser — full spec (complexType, simpleType, sequence, choice, all, extension, restriction, imports)
- [ ] SOAP 1.2 envelope parsing and serialization
- [ ] Document/literal binding dispatch (body element name -> handler)
- [ ] WS-Security UsernameToken (PasswordDigest + PasswordText + timestamp + nonce replay)
- [ ] Auth bypass for specific operations (e.g., GetSystemDateAndTime)
- [ ] WSDL serving on GET ?wsdl with address rewriting
- [ ] SOAP 1.2 fault generation
- [ ] axum Router integration (composable with other routes)
- [ ] Raw handler trait (receives XML bytes, returns XML bytes or SoapFault)
- [ ] SOAP 1.1 envelope support (backfill)
- [ ] RPC/encoded binding style (backfill)
- [ ] SOAP 1.1 fault format (backfill)
- [ ] MTOM/XOP support (backfill)
- [ ] Multiple services per WSDL (backfill)
- [ ] Typed handler API (research needed — may use yaserde or quick-xml deserialization)

### Out of Scope

- SOAP client functionality — this is a server crate only
- ONVIF-specific logic — lives in onvif-server crate
- Code generation from WSDL — runtime dispatch only
- WS-Security beyond UsernameToken (X.509, SAML, Kerberos)

## Context

Part of the Fovealink project — an ONVIF PTZ proxy for Reolink cameras. Dependency chain: `soap-server` <- `onvif-server` <- `fovealink`. Full system design at `~/ws/fovealink/docs/superpowers/specs/2026-04-03-fovealink-design.md`.

Porting primarily from python-zeep (MIT) for WSDL/XSD parsing logic. Supplementary references: node-soap (dispatch pattern), rpos (WS-Security digest), globusdigital/soap Go (minimal dispatch).

Key dependencies: roxmltree (WSDL/XSD DOM parsing at startup), quick-xml (per-request streaming), axum + tokio (HTTP/async), sha1 + base64 + chrono (WS-Security).

License: MIT OR Apache-2.0 (dual, standard Rust convention).

## Constraints

- **Spec compliance**: Must follow WSDL 1.1 and SOAP 1.1/1.2 specifications — not a partial or "good enough" implementation
- **Performance**: Per-request path uses quick-xml streaming, not DOM — WSDL/XSD parsing via roxmltree is startup-only
- **Ecosystem fit**: Must compose cleanly as an axum Router so consumers can add their own routes alongside SOAP endpoints
- **First consumer**: onvif-server needs priority 8 items before it can start — these are the critical path

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| roxmltree for WSDL/XSD, quick-xml for runtime | DOM is fine at startup; streaming needed per-request | — Pending |
| Port from python-zeep | Best-structured open-source WSDL/XSD implementation, MIT licensed | — Pending |
| axum as HTTP framework | Dominant Rust web framework, composes via Router | — Pending |
| Raw handler as primary API | Typed handler needs research; raw unblocks consumers immediately | — Pending |
| Two-pass parse pattern (parse + resolve) | Handles forward references cleanly, proven in zeep | — Pending |

---
*Last updated: 2026-04-03 after initialization*
