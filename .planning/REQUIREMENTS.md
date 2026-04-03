# Requirements: soap-server

**Defined:** 2026-04-03
**Core Value:** Given a WSDL file and handler functions, serve a fully spec-compliant SOAP endpoint with correct envelope parsing, dispatch, fault generation, and WSDL serving.

## v1 Requirements

Requirements for initial release. Each maps to roadmap phases.

### WSDL Parsing

- [ ] **WSDL-01**: Parser reads WSDL 1.1 XML and constructs in-memory representation (services, port types, bindings, messages, operations)
- [ ] **WSDL-02**: Two-pass resolution — parse pass collects raw nodes, resolve pass wires cross-references (message refs, port type refs, binding refs)
- [ ] **WSDL-03**: Import resolution — recursively loads `<wsdl:import>` targets, caches by namespace/location, handles diamond imports and prevents cycles
- [ ] **WSDL-04**: WSDL serving on GET `?wsdl` returns the WSDL XML with `soap:address location` rewritten to match the server's actual URL
- [ ] **WSDL-05**: Imported XSD schemas are either inlined or served at their own URLs via GET

### XSD Schema

- [ ] **XSD-01**: Parser reads XSD schemas and constructs in-memory type graph (elements, complexType, simpleType, attributes, groups)
- [ ] **XSD-02**: Supports `xs:sequence`, `xs:all`, `xs:choice` content models
- [ ] **XSD-03**: Supports `xs:extension` and `xs:restriction` (type inheritance) with recursive chain resolution
- [ ] **XSD-04**: Supports `xs:import` and `xs:include` with cycle detection and caching
- [ ] **XSD-05**: Supports `xs:element` with ref, minOccurs, maxOccurs, nillable, default, fixed
- [ ] **XSD-06**: Supports `xs:attribute` and `xs:attributeGroup` with ref, use, default, fixed
- [ ] **XSD-07**: Supports `xs:group` for reusable content groups
- [ ] **XSD-08**: Supports `xs:any` and `xs:anyAttribute` extensibility points
- [ ] **XSD-09**: Supports `xs:simpleType` restrictions (enumeration, minInclusive, maxInclusive, pattern, length, etc.)
- [ ] **XSD-10**: Supports `xs:list` and `xs:union` compound simple types
- [ ] **XSD-11**: Payload validation — validate request body XML against the operation's input XSD schema before handler invocation

### SOAP Envelope

- [ ] **ENV-01**: Parse SOAP 1.2 envelope — extract Header children and Body first child element
- [ ] **ENV-02**: Serialize SOAP 1.2 response envelope wrapping handler output
- [ ] **ENV-03**: Detect SOAP version from request Content-Type (`application/soap+xml` = 1.2, `text/xml` = 1.1)
- [ ] **ENV-04**: Set correct response Content-Type header matching the request's SOAP version
- [ ] **ENV-05**: Parse SOAP 1.1 envelope (backfill)
- [ ] **ENV-06**: Serialize SOAP 1.1 response envelope (backfill)

### Fault Generation

- [ ] **FLT-01**: Generate spec-correct SOAP 1.2 faults with Code/Value, Reason/Text, and optional Detail
- [ ] **FLT-02**: Support standard fault codes: VersionMismatch, MustUnderstand, DataEncodingUnknown, Sender, Receiver
- [ ] **FLT-03**: Return HTTP 500 for SOAP 1.2 faults (per W3C SOAP 1.2 spec Section 7.4.2)
- [ ] **FLT-04**: Generate spec-correct SOAP 1.1 faults with faultcode, faultstring, faultactor, detail (backfill)
- [ ] **FLT-05**: Map fault codes between versions (Sender ↔ Client, Receiver ↔ Server) (backfill)

### Dispatch

- [ ] **DSP-01**: Document/literal dispatch — route requests by body element QName (namespace + local name) to registered handler
- [ ] **DSP-02**: SOAPAction header used as secondary dispatch hint when body element alone is ambiguous
- [ ] **DSP-03**: Dispatch table built at startup from parsed WSDL — no per-request WSDL interpretation
- [ ] **DSP-04**: Unmatched operations produce a SOAP Fault (action not supported)
- [ ] **DSP-05**: RPC/encoded binding style dispatch (backfill)
- [ ] **DSP-06**: Multiple services per WSDL — dispatch across services, each with its own operation table (backfill)

### Handler API

- [ ] **HDL-01**: Raw handler trait — receives XML bytes of request body element, returns XML bytes of response body element or SoapFault
- [ ] **HDL-02**: Async handler support (handler returns a Future)
- [ ] **HDL-03**: Closure-based handler registration for ergonomic API

### Security

- [ ] **SEC-01**: Extract `wsse:Security` header from SOAP Header
- [ ] **SEC-02**: WS-Security UsernameToken PasswordDigest validation — `Base64(SHA-1(Base64Decode(Nonce) + Created + Password))`
- [ ] **SEC-03**: WS-Security UsernameToken PasswordText validation — direct plaintext comparison
- [ ] **SEC-04**: Timestamp validation with configurable tolerance (default 300 seconds)
- [ ] **SEC-05**: Nonce replay prevention with time-windowed cache (rotating bucket design, default 300s window)
- [ ] **SEC-06**: Per-operation auth bypass — configurable whitelist of operations that skip WS-Security
- [ ] **SEC-07**: Reject unauthenticated requests with SOAP Fault on auth failure

### HTTP Integration

- [ ] **HTTP-01**: axum Router integration — server returns `axum::Router` composable with other routes
- [ ] **HTTP-02**: POST handler for SOAP requests on configured path
- [ ] **HTTP-03**: GET handler for WSDL serving on same path with `?wsdl` query parameter
- [ ] **HTTP-04**: Server builder API — `Server::from_wsdl(wsdl).handler(...).auth(...).build()?`

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

### Advanced Features

- **ADV-01**: MTOM/XOP support — multipart MIME parsing, XOP Include element replacement for binary attachments
- **ADV-02**: Typed handler API — WSDL-derived type deserialization/serialization (needs research on yaserde vs quick-xml approach)
- **ADV-03**: WS-Addressing support — enterprise routing headers

### WS-Security (Extended)

- **SEC-10**: WS-Security X.509 certificate token support
- **SEC-11**: WS-Security SAML token support
- **SEC-12**: WS-Security Kerberos token support
- **SEC-13**: WS-SecurityPolicy integration

## Out of Scope

| Feature | Reason |
|---------|--------|
| SOAP client functionality | Server-only crate — client is a separate concern, not part of the SOAP server spec |
| Code generation from WSDL (proc-macro / build.rs) | Developer ergonomics layer, not a spec requirement — runtime dispatch covers all spec functionality |
| WSDL generation from Rust types | Not part of SOAP server spec — SOAP is contract-first, WSDL is the authority |
| WS-Security beyond UsernameToken (X.509, SAML, Kerberos) | Deferred to v2 — substantial implementations, UsernameToken covers ONVIF and most enterprise use cases first |
| Built-in TLS/HTTPS | Transport-layer concern, not SOAP spec — axum handles via `axum-server` or reverse proxy |
| Session / connection state management | Not part of SOAP spec — SOAP is stateless by design |
| Dynamic WSDL generation from schema | Not part of SOAP server spec — server serves a provided WSDL, doesn't synthesize one |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| WSDL-01 | Phase 1 | Pending |
| WSDL-02 | Phase 1 | Pending |
| WSDL-03 | Phase 1 | Pending |
| WSDL-04 | Phase 1 | Pending |
| WSDL-05 | Phase 1 | Pending |
| XSD-01 | Phase 1 | Pending |
| XSD-02 | Phase 1 | Pending |
| XSD-03 | Phase 1 | Pending |
| XSD-04 | Phase 1 | Pending |
| XSD-05 | Phase 1 | Pending |
| XSD-06 | Phase 1 | Pending |
| XSD-07 | Phase 1 | Pending |
| XSD-08 | Phase 1 | Pending |
| XSD-09 | Phase 1 | Pending |
| XSD-10 | Phase 1 | Pending |
| XSD-11 | Phase 1 | Pending |
| ENV-01 | Phase 1 | Pending |
| ENV-02 | Phase 1 | Pending |
| ENV-03 | Phase 1 | Pending |
| ENV-04 | Phase 1 | Pending |
| ENV-05 | Phase 2 | Pending |
| ENV-06 | Phase 2 | Pending |
| FLT-01 | Phase 1 | Pending |
| FLT-02 | Phase 1 | Pending |
| FLT-03 | Phase 1 | Pending |
| FLT-04 | Phase 2 | Pending |
| FLT-05 | Phase 2 | Pending |
| DSP-01 | Phase 1 | Pending |
| DSP-02 | Phase 1 | Pending |
| DSP-03 | Phase 1 | Pending |
| DSP-04 | Phase 1 | Pending |
| DSP-05 | Phase 2 | Pending |
| DSP-06 | Phase 2 | Pending |
| HDL-01 | Phase 1 | Pending |
| HDL-02 | Phase 1 | Pending |
| HDL-03 | Phase 1 | Pending |
| SEC-01 | Phase 1 | Pending |
| SEC-02 | Phase 1 | Pending |
| SEC-03 | Phase 1 | Pending |
| SEC-04 | Phase 1 | Pending |
| SEC-05 | Phase 1 | Pending |
| SEC-06 | Phase 1 | Pending |
| SEC-07 | Phase 1 | Pending |
| HTTP-01 | Phase 1 | Pending |
| HTTP-02 | Phase 1 | Pending |
| HTTP-03 | Phase 1 | Pending |
| HTTP-04 | Phase 1 | Pending |

**Coverage:**
- v1 requirements: 47 total
- Mapped to phases: 47
- Unmapped: 0 ✓

---
*Requirements defined: 2026-04-03*
*Last updated: 2026-04-03 — traceability updated after 4-phase to 2-phase restructure*
