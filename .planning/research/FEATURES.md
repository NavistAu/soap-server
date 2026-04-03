# Feature Research

**Domain:** SOAP server library (Rust crate)
**Researched:** 2026-04-03
**Confidence:** HIGH (SOAP spec is stable; feature expectations drawn from Apache CXF, Spring-WS, node-soap, PHP SoapServer — all well-documented production implementations)

## Feature Landscape

### Table Stakes (Users Expect These)

Features users assume any SOAP server library provides. Missing these = library is unusable for production.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| SOAP envelope parsing (1.1 + 1.2) | Every SOAP client speaks one of these two versions; a server that only handles one will break against real-world clients | MEDIUM | Envelopes differ structurally: namespace, fault element names, Content-Type header (`text/xml` vs `application/soap+xml`) |
| WSDL 1.1 serving on GET `?wsdl` | Clients auto-discover the contract; SOAP tooling (SoapUI, wsdl2code) requires the WSDL endpoint | LOW | Must rewrite `soap:address` to reflect the actual request URL, not whatever was in the static file |
| Document/literal dispatch (body element → handler) | The dominant binding style in production WSDLs; WS-I Basic Profile mandates it; RPC/encoded is explicitly non-compliant | HIGH | Requires WSDL parse to build operation table keyed on body element local name + namespace |
| SOAP fault generation (spec-correct structure) | Any unhandled error must produce a valid Fault element or the client receives unparseable XML; tooling tests fault structure | MEDIUM | SOAP 1.1: `faultcode`/`faultstring`/`faultactor`/`detail`; SOAP 1.2: `Code`/`Reason`/`Role`/`Detail` — different schemas, both required |
| XSD schema parsing (inline + imported) | WSDL types sections reference XSD; dispatch table cannot be built without resolving them | HIGH | Must handle `complexType`, `simpleType`, `sequence`, `choice`, `all`, `extension`, `restriction`, `import` — forward references require two-pass |
| HTTP 500 on fault | SOAP 1.1 spec requires HTTP 500 for faults; SOAP 1.2 requires 400 or 500 depending on fault code | LOW | Wrong status code breaks interop with strict clients |
| Content-Type validation and response headers | Clients set `Content-Type: text/xml` (1.1) or `application/soap+xml` (1.2); server response headers must match | LOW | Mismatch causes parse failures in strict clients |
| Operation routing by SOAPAction header (SOAP 1.1) | SOAP 1.1 clients often send `SOAPAction` header; some dispatch on it rather than body element | LOW | Secondary dispatch hint; body element name is canonical |
| Single-service WSDL support | Minimum functional unit; all real clients have at least one service | MEDIUM | Multiple bindings and ports within one service already required |

### Differentiators (Competitive Advantage)

Features that make this crate stand out versus a hand-rolled SOAP handler or the thin alternatives in the Rust ecosystem.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| WS-Security UsernameToken (PasswordDigest + PasswordText) | ONVIF mandates it; many enterprise SOAP services require it; nothing else in Rust provides it | HIGH | SHA-1(nonce + created + password), base64 encoding, timestamp window validation (300s default), nonce replay prevention with deduplication cache |
| Per-operation auth bypass | ONVIF pattern: `GetSystemDateAndTime` must be unauthenticated so client can sync clocks before calculating digests | LOW | Whitelist of operation names that skip WS-Security validation |
| axum Router integration | Consumers can mount SOAP alongside REST routes — standard Rust web server pattern; no custom HTTP stack needed | MEDIUM | Returns `axum::Router`; handles GET `?wsdl` and POST on same path |
| Raw handler trait (XML bytes in, XML bytes out) | Unblocks consumers immediately without waiting for typed deserialization; allows any XML library in the handler | LOW | Trait: `fn handle(&self, body: &[u8]) -> Result<Vec<u8>, SoapFault>` |
| MTOM/XOP support | Required for camera streams, binary file transfers; ONVIF uses it for snapshot delivery | HIGH | Multipart MIME parsing; XOP Include element replacement; large binary data outside the envelope |
| Multiple services per WSDL | Enterprise WSDLs routinely define 2–5 services; failing on them is a library deficiency | MEDIUM | Router must dispatch across services; each service gets its own operation table |
| SOAP 1.1 + 1.2 auto-detection per request | Client sends the version it speaks; server should accept both on the same endpoint | MEDIUM | Detect from Content-Type header; select correct fault format and namespace in response |
| Typed handler API (WSDL-derived types) | Eliminates the user's need to manually write XSD-matching Rust structs and XML serialization; true DX advantage | HIGH | Requires runtime XSD-to-type mapping + `yaserde` or `quick-xml` deserialization; research needed on approach |
| Payload validation against XSD | Rejects malformed requests before they reach handler code; enterprise quality gate | MEDIUM | Optional interceptor-style; validate request body against the operation's input schema |
| RPC/encoded binding style | Legacy interop with older Java/.NET services that haven't migrated to document/literal | MEDIUM | Not WS-I compliant but still widely deployed in enterprise environments |
| SOAP 1.1 fault format (backfill) | Complete fault coverage for SOAP 1.1 clients | LOW | Separate fault serialization path for 1.1 vs 1.2 |

### Anti-Features (Commonly Requested, Often Problematic)

Features that seem desirable but should be deliberately excluded from this crate.

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| SOAP client functionality | "One crate that does everything" is appealing | Entirely different code path, different API contract, doubles maintenance scope, confuses crate purpose; all existing Rust SOAP gaps are server-side | Ship server crate; SOAP clients can use `savon` or `soafe` |
| Code generation from WSDL (proc-macro / build.rs) | Typed Rust structs from WSDL feel ergonomic | Compile-time codegen couples WSDL schema changes to recompilation; complex to implement correctly; creates a dependency on build tooling version; runtime dispatch is more flexible | Runtime dispatch with optional typed handler layer on top |
| WSDL generation from Rust types | "Code-first" approach: define Rust structs, emit WSDL | Inverts the contract: WSDL is the authority; code-first WSDLs are often non-interoperable; SOAP ecosystem is contract-first by convention | Require consumers to supply a WSDL; document-first is the correct model |
| WS-Security beyond UsernameToken (X.509, SAML, Kerberos) | Comprehensive security coverage | Each mechanism is a substantial implementation effort; X.509 requires PKI infrastructure; SAML needs identity provider integration; scope creep relative to primary use case | Provide extension hooks for custom security headers; document what WS-Security variants are out of scope |
| Built-in TLS/HTTPS | Simplifies deployment | Rust async TLS has its own complexity; `rustls` and `native-tls` are established; axum handles TLS via `axum-server` or reverse proxy | Document that TLS should be handled by the HTTP layer (axum + rustls, or a reverse proxy) |
| Dynamic WSDL generation from schema | Auto-generate WSDL at runtime from XSD | Adds a separate WSDL synthesis step; generated WSDLs are often subtly wrong for interop; WSDL structure beyond types (bindings, ports, services) cannot be inferred from XSD alone | Require a static WSDL file; server serves it as-is with address rewriting |
| Session / connection state management | Stateful SOAP sessions (WS-Session) | SOAP is fundamentally stateless; session state belongs in the application layer; adding it to the transport layer creates coupling | Handlers are stateless; application-level state lives in handler closures or shared `Arc<State>` |

## Feature Dependencies

```
XSD schema parsing
    └──required by──> WSDL 1.1 parsing (types section references XSD)
                          └──required by──> Document/literal dispatch (operation table)
                          └──required by──> WSDL serving on GET ?wsdl (needs complete WSDL)
                          └──required by──> Payload validation against XSD

SOAP 1.1 envelope parsing
    └──required by──> SOAP 1.1 fault format
    └──required by──> SOAPAction dispatch (header only present in 1.1)

SOAP 1.2 envelope parsing
    └──required by──> SOAP 1.2 fault generation

Document/literal dispatch
    └──required by──> Raw handler trait (dispatch routes to handler)
    └──required by──> Typed handler API (dispatch routes to typed handler)
    └──required by──> Per-operation auth bypass (needs resolved operation name)

WS-Security UsernameToken
    └──required by──> Per-operation auth bypass (bypass is per-operation exception to WS-Security)
    └──enhanced by──> Nonce replay cache (stateful deduplication, prevents replay attacks)

MTOM/XOP support
    └──requires──> SOAP envelope parsing (MTOM messages are still SOAP; XOP replaces inline content)

Typed handler API
    └──requires──> XSD schema parsing (need type information to deserialize)
    └──enhances──> Raw handler trait (typed builds on top of raw dispatch infrastructure)

axum Router integration
    └──requires──> SOAP envelope parsing (router handles HTTP, passes to SOAP layer)
    └──requires──> WSDL serving (router handles GET ?wsdl)
```

### Dependency Notes

- **XSD parsing requires two-pass:** Forward references (type A uses type B defined later in the file) require a first pass to collect all declarations and a second pass to resolve references. Python-zeep's approach is the reference implementation.
- **Typed handler API is independent research:** Whether to use `yaserde`, `quick-xml` serde-style, or a custom approach is unresolved. Raw handler trait is the unblocking primitive; typed layer can be added without changing dispatch infrastructure.
- **MTOM conflicts with streaming XML parse for request bodies:** MTOM bodies are multipart MIME, not pure XML; must detect `Content-Type: multipart/related` before routing to the SOAP XML parser. This is a distinct code path.
- **SOAP 1.1 and 1.2 fault formats are mutually exclusive per request:** The response fault must match the version detected in the incoming request envelope.

## MVP Definition

### Launch With (v1)

Minimum needed to unblock `onvif-server` and validate the library design.

- [ ] XSD schema parser (full spec) — everything else depends on it
- [ ] WSDL 1.1 parser with two-pass resolution — required before dispatch table can be built
- [ ] SOAP 1.2 envelope parsing — ONVIF mandates SOAP 1.2
- [ ] Document/literal dispatch (body element → handler) — primary dispatch model
- [ ] Raw handler trait — unblocks onvif-server immediately
- [ ] SOAP 1.2 fault generation — required for error responses
- [ ] WSDL serving on GET `?wsdl` with address rewriting — required for client discovery
- [ ] WS-Security UsernameToken (PasswordDigest + timestamp + nonce replay) — ONVIF security requirement
- [ ] Per-operation auth bypass — ONVIF GetSystemDateAndTime pattern
- [ ] axum Router integration — how consumers mount the SOAP endpoint

### Add After Validation (v1.x)

Add once onvif-server is working and library API is proven stable.

- [ ] SOAP 1.1 envelope support — needed for broader ecosystem compatibility beyond ONVIF
- [ ] SOAP 1.1 fault format — paired with 1.1 envelope support
- [ ] RPC/encoded binding style — legacy enterprise interop
- [ ] Multiple services per WSDL — enterprise WSDLs with multiple service definitions
- [ ] MTOM/XOP support — binary attachment handling (camera snapshots)
- [ ] Payload validation against XSD — optional quality gate for handler input

### Future Consideration (v2+)

Defer until v1.x is validated and there is consumer demand.

- [ ] Typed handler API — high value but requires separate research spike on deserialization approach; raw handler covers all functionality
- [ ] WS-Addressing — enterprise routing headers; not required for ONVIF
- [ ] WS-Security PasswordText mode — less secure variant of UsernameToken; low priority

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| XSD schema parsing | HIGH | HIGH | P1 |
| WSDL 1.1 parsing (two-pass) | HIGH | HIGH | P1 |
| SOAP 1.2 envelope parsing | HIGH | MEDIUM | P1 |
| Document/literal dispatch | HIGH | MEDIUM | P1 |
| Raw handler trait | HIGH | LOW | P1 |
| SOAP 1.2 fault generation | HIGH | LOW | P1 |
| WSDL serving + address rewrite | HIGH | LOW | P1 |
| WS-Security UsernameToken | HIGH | HIGH | P1 |
| Per-operation auth bypass | HIGH | LOW | P1 |
| axum Router integration | HIGH | MEDIUM | P1 |
| SOAP 1.1 envelope + fault | MEDIUM | MEDIUM | P2 |
| RPC/encoded dispatch | MEDIUM | MEDIUM | P2 |
| Multiple services per WSDL | MEDIUM | MEDIUM | P2 |
| MTOM/XOP support | MEDIUM | HIGH | P2 |
| Payload XSD validation | MEDIUM | MEDIUM | P2 |
| Typed handler API | HIGH | HIGH | P3 |
| WS-Addressing | LOW | MEDIUM | P3 |

**Priority key:**
- P1: Must have for launch (onvif-server unblocked)
- P2: Should have, add when possible (broader ecosystem compatibility)
- P3: Nice to have, future consideration

## Competitor Feature Analysis

| Feature | Apache CXF / Spring-WS (Java) | node-soap (Node.js) | PHP SoapServer | Our Approach |
|---------|-------------------------------|---------------------|----------------|--------------|
| Dispatch model | Annotation-based (`@PayloadRoot`, `@SoapAction`) or XML-level | Service object with method names matching WSDL operations | PHP function names matching WSDL operations | Body element local name → handler function via Rust trait |
| WSDL handling | Contract-first preferred; dynamic WSDL generation from XSD | Parses WSDL at startup; serves on request | PHP parses WSDL; no address rewriting | Static WSDL parse at startup; serve with address rewrite |
| WS-Security | Full WS-SecurityPolicy; X.509, SAML, Kerberos, UsernameToken | UsernameToken only | Extension via custom headers | UsernameToken (PasswordDigest + PasswordText); others out of scope |
| Fault generation | Annotation-driven (`@SoapFault`); exception-to-fault mapping | Throw object with `.Fault` property | Return fault array | Rust `Result<_, SoapFault>` with version-aware serialization |
| Envelope versions | 1.1 and 1.2 | 1.1 default; 1.2 via `forceSoap12Headers` | 1.1 only (PHP ext) | 1.2 first; 1.1 backfill |
| Binding styles | Document/literal (primary); RPC/encoded (legacy) | Both; document/literal preferred | Both | Document/literal first; RPC/encoded backfill |
| Interceptors/middleware | Rich interceptor chain (logging, validation, security, transform) | Event emitter (`request`, `headers` events) | None | Axum middleware layer + operation-level hooks |
| MTOM | Full support | Partial | No | Backfill after v1 |
| Typed handler API | Full OXM (JAXB, Castor, etc.) | JavaScript objects from WSDL | PHP native types | Research needed; raw handler is v1 |
| HTTP framework coupling | Servlet container (Tomcat/Jetty) or Spring Boot | Express.js | PHP built-in | axum Router (composable) |

## Sources

- [node-soap GitHub repository](https://github.com/vpulim/node-soap) — server API, event model, WS-Security, fault handling
- [Spring-WS server reference](https://docs.spring.io/spring-ws/sites/2.0/reference/html/server.html) — interceptor chain, endpoint mapping, WSDL handling, fault generation
- [Apache CXF WS-Security](https://cxf.apache.org/docs/securing-cxf-services.html) — WS-Security token types, binding styles
- [WSDL binding styles (IBM)](https://developer.ibm.com/articles/ws-whichwsdl/) — document/literal vs RPC/encoded compliance
- [ONVIF Core Specification](https://www.onvif.org/specs/core/ONVIF-Core-Specification.pdf) — SOAP 1.2 document/literal mandate, UsernameToken requirements, GetSystemDateAndTime auth bypass pattern
- [SOAP Fault structures](https://www.informit.com/articles/article.aspx?p=327825&seqNum=11) — SOAP 1.1 vs 1.2 fault element differences
- [MTOM + WS-Security interaction](https://docs.oasis-open.org/wss-m/wss/v1.1.1/os/wss-SwAProfile-v1.1.1-os.html) — MIME multipart + security header interaction
- [Rust SOAP ecosystem (crates.io)](https://crates.io/search?q=soap) — soafe, savon, soap-service, wsdl crates; confirms server gap

---
*Feature research for: Rust SOAP server crate*
*Researched: 2026-04-03*
