# Requirements: soap-server

**Defined:** 2026-04-03
**Core Value:** Given a WSDL file and handler functions, serve a fully spec-compliant SOAP endpoint with correct envelope parsing, dispatch, fault generation, and WSDL serving.

## v1 Requirements

Requirements for initial release. Each maps to roadmap phases.

### WSDL Parsing

- [x] **WSDL-01**: Parser reads WSDL 1.1 XML and constructs in-memory representation (services, port types, bindings, messages, operations)
- [x] **WSDL-02**: Two-pass resolution — parse pass collects raw nodes, resolve pass wires cross-references (message refs, port type refs, binding refs)
- [x] **WSDL-03**: Import resolution — recursively loads `<wsdl:import>` targets, caches by namespace/location, handles diamond imports and prevents cycles
- [x] **WSDL-04**: WSDL serving on GET `?wsdl` returns the WSDL XML with `soap:address location` rewritten to match the server's actual URL
- [x] **WSDL-05**: Imported XSD schemas are either inlined or served at their own URLs via GET

### XSD Schema

- [x] **XSD-01**: Parser reads XSD schemas and constructs in-memory type graph (elements, complexType, simpleType, attributes, groups)
- [x] **XSD-02**: Supports `xs:sequence`, `xs:all`, `xs:choice` content models
- [x] **XSD-03**: Supports `xs:extension` and `xs:restriction` (type inheritance) with recursive chain resolution
- [x] **XSD-04**: Supports `xs:import` and `xs:include` with cycle detection and caching
- [x] **XSD-05**: Supports `xs:element` with ref, minOccurs, maxOccurs, nillable, default, fixed
- [x] **XSD-06**: Supports `xs:attribute` and `xs:attributeGroup` with ref, use, default, fixed
- [x] **XSD-07**: Supports `xs:group` for reusable content groups
- [x] **XSD-08**: Supports `xs:any` and `xs:anyAttribute` extensibility points
- [x] **XSD-09**: Supports `xs:simpleType` restrictions (enumeration, minInclusive, maxInclusive, pattern, length, etc.)
- [x] **XSD-10**: Supports `xs:list` and `xs:union` compound simple types
- [x] **XSD-11**: Payload validation — validate request body XML against the operation's input XSD schema before handler invocation

### SOAP Envelope

- [x] **ENV-01**: Parse SOAP 1.2 envelope — extract Header children and Body first child element
- [x] **ENV-02**: Serialize SOAP 1.2 response envelope wrapping handler output
- [x] **ENV-03**: Detect SOAP version from request Content-Type (`application/soap+xml` = 1.2, `text/xml` = 1.1)
- [x] **ENV-04**: Set correct response Content-Type header matching the request's SOAP version
- [ ] **ENV-05**: Parse SOAP 1.1 envelope (backfill)
- [ ] **ENV-06**: Serialize SOAP 1.1 response envelope (backfill)

### Fault Generation

- [x] **FLT-01**: Generate spec-correct SOAP 1.2 faults with Code/Value, Reason/Text, and optional Detail
- [x] **FLT-02**: Support standard fault codes: VersionMismatch, MustUnderstand, DataEncodingUnknown, Sender, Receiver
- [x] **FLT-03**: Return HTTP 500 for SOAP 1.2 faults (per W3C SOAP 1.2 spec Section 7.4.2)
- [ ] **FLT-04**: Generate spec-correct SOAP 1.1 faults with faultcode, faultstring, faultactor, detail (backfill)
- [ ] **FLT-05**: Map fault codes between versions (Sender ↔ Client, Receiver ↔ Server) (backfill)

### Dispatch

- [x] **DSP-01**: Document/literal dispatch — route requests by body element QName (namespace + local name) to registered handler
- [x] **DSP-02**: SOAPAction header used as secondary dispatch hint when body element alone is ambiguous
- [x] **DSP-03**: Dispatch table built at startup from parsed WSDL — no per-request WSDL interpretation
- [x] **DSP-04**: Unmatched operations produce a SOAP Fault (action not supported)
- [x] **DSP-05**: RPC/encoded binding style dispatch (backfill)
- [x] **DSP-06**: Multiple services per WSDL — dispatch across services, each with its own operation table (backfill)

### Handler API

- [x] **HDL-01**: Raw handler trait — receives XML bytes of request body element, returns XML bytes of response body element or SoapFault
- [x] **HDL-02**: Async handler support (handler returns a Future)
- [x] **HDL-03**: Closure-based handler registration for ergonomic API

### Security

- [x] **SEC-01**: Extract `wsse:Security` header from SOAP Header
- [x] **SEC-02**: WS-Security UsernameToken PasswordDigest validation — `Base64(SHA-1(Base64Decode(Nonce) + Created + Password))`
- [x] **SEC-03**: WS-Security UsernameToken PasswordText validation — direct plaintext comparison
- [x] **SEC-04**: Timestamp validation with configurable tolerance (default 300 seconds)
- [x] **SEC-05**: Nonce replay prevention with time-windowed cache (rotating bucket design, default 300s window)
- [x] **SEC-06**: Per-operation auth bypass — configurable whitelist of operations that skip WS-Security
- [x] **SEC-07**: Reject unauthenticated requests with SOAP Fault on auth failure

### HTTP Integration

- [x] **HTTP-01**: axum Router integration — server returns `axum::Router` composable with other routes
- [x] **HTTP-02**: POST handler for SOAP requests on configured path
- [x] **HTTP-03**: GET handler for WSDL serving on same path with `?wsdl` query parameter
- [x] **HTTP-04**: Server builder API — `Server::from_wsdl(wsdl).handler(...).auth(...).build()?`

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
| WSDL-01 | Phase 1 | Complete |
| WSDL-02 | Phase 1 | Complete |
| WSDL-03 | Phase 1 | Complete |
| WSDL-04 | Phase 1 | Complete |
| WSDL-05 | Phase 1 | Complete |
| XSD-01 | Phase 1 | Complete |
| XSD-02 | Phase 1 | Complete |
| XSD-03 | Phase 1 | Complete |
| XSD-04 | Phase 1 | Complete |
| XSD-05 | Phase 1 | Complete |
| XSD-06 | Phase 1 | Complete |
| XSD-07 | Phase 1 | Complete |
| XSD-08 | Phase 1 | Complete |
| XSD-09 | Phase 1 | Complete |
| XSD-10 | Phase 1 | Complete |
| XSD-11 | Phase 1 | Complete |
| ENV-01 | Phase 1 | Complete |
| ENV-02 | Phase 1 | Complete |
| ENV-03 | Phase 1 | Complete |
| ENV-04 | Phase 1 | Complete |
| ENV-05 | Phase 2 | Pending |
| ENV-06 | Phase 2 | Pending |
| FLT-01 | Phase 1 | Complete |
| FLT-02 | Phase 1 | Complete |
| FLT-03 | Phase 1 | Complete |
| FLT-04 | Phase 2 | Pending |
| FLT-05 | Phase 2 | Pending |
| DSP-01 | Phase 1 | Complete |
| DSP-02 | Phase 1 | Complete |
| DSP-03 | Phase 1 | Complete |
| DSP-04 | Phase 1 | Complete |
| DSP-05 | Phase 2 | Complete |
| DSP-06 | Phase 2 | Complete |
| HDL-01 | Phase 1 | Complete |
| HDL-02 | Phase 1 | Complete |
| HDL-03 | Phase 1 | Complete |
| SEC-01 | Phase 1 | Complete |
| SEC-02 | Phase 1 | Complete |
| SEC-03 | Phase 1 | Complete |
| SEC-04 | Phase 1 | Complete |
| SEC-05 | Phase 1 | Complete |
| SEC-06 | Phase 1 | Complete |
| SEC-07 | Phase 1 | Complete |
| HTTP-01 | Phase 1 | Complete |
| HTTP-02 | Phase 1 | Complete |
| HTTP-03 | Phase 1 | Complete |
| HTTP-04 | Phase 1 | Complete |

**Coverage:**
- v1 requirements: 47 total
- Mapped to phases: 47
- Unmapped: 0 ✓

---
*Requirements defined: 2026-04-03*
*Last updated: 2026-04-03 — traceability updated after 4-phase to 2-phase restructure*
