# Project Research Summary

**Project:** soap-server
**Domain:** Rust library crate — general-purpose SOAP server (WSDL/XSD parsing, HTTP dispatch, WS-Security)
**Researched:** 2026-04-03
**Confidence:** HIGH

## Executive Summary

The Rust SOAP server ecosystem is effectively empty. No production-grade SOAP server crate exists on crates.io as of April 2026 — the closest candidate (`soap-service` 0.2.1) has 1,415 total downloads and is a thin macro wrapper with minimal adoption. This project is genuinely greenfield with no prior art to build on, which means we control the API design but also bear the full implementation burden. The recommended approach is to assemble a server from purpose-fit components: roxmltree for startup WSDL/XSD DOM traversal, quick-xml for per-request envelope streaming, and axum for HTTP integration. The primary validation target is the ONVIF device API, which mandates SOAP 1.2 document/literal with WS-Security UsernameToken PasswordDigest.

The architecture has two fundamentally distinct phases that must be kept separate: a startup phase that parses WSDL/XSD files into an immutable `Arc<ServiceModel>` and builds a dispatch table, and a per-request phase that streams the incoming envelope, validates WS-Security headers, dispatches to a handler by body element QName, and wraps the response. The most critical design decision is two-pass WSDL/XSD parsing — a single-pass parser cannot correctly handle forward references and cross-file imports, both of which appear in real-world ONVIF WSDLs. Skipping the two-pass design is not a shortcut; it is a correctness failure.

The top risks are all specification traps: namespace context loss when extracting XML fragments across parser boundaries, incomplete XSD inheritance chain resolution, SOAP version namespace confusion, and PasswordDigest byte-order errors (nonce must be Base64-decoded before hashing, not concatenated as a string). Every one of these produces authentication or dispatch failures that are difficult to debug after the fact. The prevention strategy is the same in each case: write test fixtures with known input/output values before implementing the logic.

## Key Findings

### Recommended Stack

The stack is built around a clear division between startup and per-request paths. roxmltree (0.21.1) is the right choice for WSDL/XSD parsing at startup — it is the fastest read-only DOM library in Rust and its API is designed exactly for traversal-and-extract workflows. quick-xml (0.39.2) is the right choice for per-request SOAP envelope parsing — its SAX-style streaming avoids heap allocation on the hot path and correctly handles namespace resolution via `NsReader`. axum (0.8.8) provides the HTTP layer and composes cleanly via `Router::merge`, so consumers can mount the SOAP endpoint alongside existing REST routes. The RustCrypto suite (sha1 0.11, base64 0.22, uuid 1.23) handles WS-Security cryptography in pure Rust with no C build dependency.

No existing SOAP server crate is suitable as a foundation. All should be avoided. The project builds from scratch.

**Core technologies:**
- roxmltree 0.21.1: WSDL/XSD DOM parsing at startup — fastest read-only Rust XML DOM, zero-copy where possible
- quick-xml 0.39.2: per-request SOAP envelope streaming parse/write — 10x faster than serde-xml-rs, allocation-free hot path
- axum 0.8.8: HTTP server and router integration — dominant Rust web framework, composes via Router::merge
- tokio 1.50.0: async runtime — required by axum; `tokio::sync::Mutex` for nonce cache
- sha1 + base64 + uuid (RustCrypto): WS-Security PasswordDigest computation — pure Rust, no C build step
- thiserror 2.0.18: idiomatic error handling — derive Error for SoapFault and internal error types

### Expected Features

The feature dependency tree has a single critical spine: XSD parser → WSDL parser → dispatch table → everything else. Nothing can be built out of order. For v1, the target is the minimum needed to unblock an ONVIF server implementation: SOAP 1.2 envelope parsing, document/literal dispatch, raw handler trait, SOAP 1.2 fault generation, WSDL serving with address rewriting, WS-Security UsernameToken with PasswordDigest and nonce replay prevention, per-operation auth bypass, and axum router integration.

**Must have (table stakes — v1):**
- XSD schema parser with two-pass resolution — all other features depend on it
- WSDL 1.1 parser with two-pass forward-reference resolution — required for dispatch table
- SOAP 1.2 envelope parsing — ONVIF mandates SOAP 1.2
- Document/literal dispatch (body element QName → handler) — primary dispatch model; SOAPAction is secondary only
- Raw handler trait (`&[u8]` in, `Vec<u8>` / `SoapFault` out) — unblocks ONVIF server immediately
- SOAP 1.2 fault generation (spec-correct structure) — required for error responses
- WSDL serving on GET `?wsdl` with soap:address rewriting — required for client tooling discovery
- WS-Security UsernameToken PasswordDigest + timestamp freshness + nonce replay cache — ONVIF security requirement
- Per-operation auth bypass whitelist — ONVIF GetSystemDateAndTime pattern
- axum Router integration (`into_router() -> axum::Router`) — how consumers mount the endpoint

**Should have (competitive — v1.x):**
- SOAP 1.1 envelope parsing + fault format — broader ecosystem compatibility beyond ONVIF
- Multiple services per WSDL — enterprise WSDLs routinely define 2-5 services
- MTOM/XOP support — binary attachment handling (ONVIF camera snapshots)
- Payload validation against XSD — optional quality gate
- RPC/encoded binding style — legacy enterprise interop

**Defer (v2+):**
- Typed handler API (WSDL-derived Rust types) — high value but requires separate research spike; raw handler covers all functionality
- WS-Addressing — enterprise routing; not required for ONVIF
- WS-Security PasswordText mode — less secure variant; low priority

### Architecture Approach

The architecture is a layered pipeline with a strict startup/request boundary. At startup: WSDL/XSD files are parsed in two passes (parse → resolve), assembled into an immutable `Arc<ServiceModel>`, and compiled into a `DispatchTable` (QName → handler). At request time: axum receives the HTTP POST, quick-xml streams the envelope to split Header and Body, WS-Security validation runs as a pre-dispatch interceptor (with per-operation bypass), the dispatcher does an O(1) HashMap lookup on the body first-child QName, the matched handler is called with raw body bytes, and the response is wrapped in a version-correct SOAP envelope. Each layer is independently testable. The only shared mutable state is the nonce cache, protected by `tokio::sync::Mutex`.

**Major components:**
1. `xsd/` (parser + resolver + types) — parse XSD complexType/simpleType/element/attribute; resolve extension/restriction chains and cross-file imports; emit TypeRegistry keyed by QName
2. `wsdl/` (parser + resolver + definitions) — two-pass WSDL parse; resolves forward refs and schema imports; emits ServiceModel
3. `model.rs` — immutable assembled view joining WSDL + XSD output; stored as `Arc<ServiceModel>`, shared across requests at zero cost
4. `dispatch.rs` — HashMap from body element QName to `Arc<dyn SoapHandler>`; built once at startup
5. `envelope.rs` — quick-xml streaming parse of SOAP Header + Body; version-correct response envelope serialization
6. `fault.rs` — version-aware SOAP fault generation (SOAP 1.1: faultcode/faultstring; SOAP 1.2: Code/Reason)
7. `wssec/` (username_token + nonce_cache + timestamp) — PasswordDigest validation; timestamp freshness enforcement; replay prevention via rotating-bucket nonce store
8. `router.rs` — axum Router construction; wires all components; only file that imports axum; public API entry point

### Critical Pitfalls

1. **Namespace inheritance loss when extracting XML fragments** — when body bytes are passed to a handler, ancestor namespace declarations from the Envelope element are lost; the handler's parser sees unbound prefixes. Prevention: re-emit all in-scope namespace declarations on the extracted fragment root, or pass a `(bytes, namespace_map)` tuple. Address this in the envelope parsing phase before any handler API is finalized.

2. **XSD extension/restriction not fully walking the inheritance chain** — multi-level inheritance (ONVIF PTZNode extends DeviceEntity) requires recursively flattening all ancestor types during the resolve pass. Single-level implementations silently drop fields from base types. Prevention: write 3-level inheritance test fixtures before implementing resolution; follow python-zeep's `xsd/elements/complex.py` logic.

3. **PasswordDigest byte order error** — the correct formula is `Base64(SHA-1(B64DECODE(nonce) ++ created_bytes ++ password_bytes))`. Concatenating the raw Base64 nonce string instead of decoding it first produces a digest that never matches. Prevention: write a unit test with a hard-coded known-good nonce/created/password/digest tuple from the ONVIF reference implementation before writing any PasswordDigest code.

4. **SOAP 1.1 vs 1.2 namespace confusion** — SOAP 1.1 and 1.2 use different envelope namespaces, different fault structures, and different HTTP status code requirements. Hard-coding either namespace string anywhere breaks the other version silently. Prevention: define a `SoapVersion` enum detected from Content-Type before XML parsing; use version-gated namespace constants throughout.

5. **WS-Security nonce cache unbounded growth** — a plain `HashSet<String>` with no expiry causes memory growth under sustained load. Prevention: use a rotating two-bucket design (swap every T/2 seconds where T is the freshness window); a nonce seen in either bucket is rejected. Recovery cost is low (no API change) but should be built correctly from the start.

## Implications for Roadmap

The build order is dictated by the feature dependency graph. Nothing can be tested end-to-end until all layers are present. The architecture research provides an explicit build sequence that should map directly to phases.

### Phase 1: Foundation — Error Types, Fault Generation, Envelope Parsing

**Rationale:** `fault.rs` and `envelope.rs` have no upstream dependencies and are required by every subsequent layer. Building them first establishes the error model and SOAP version handling before anything else touches it. SOAP version detection (from Content-Type) must precede all parsing — this is the natural place to lock it in.

**Delivers:** Ability to parse a SOAP envelope into Header + Body bytes and generate a spec-correct fault response for either SOAP version. The SoapHandler trait definition also lives here.

**Addresses:** SOAP 1.2 envelope parsing, SOAP 1.2/1.1 fault generation, Content-Type validation, HTTP status code correctness

**Avoids:** SOAP version namespace hard-coding (Pitfall 3); namespace inheritance loss in fragment extraction (Pitfall 1 — the API boundary for handlers is decided here)

**Research flag:** Standard patterns — no additional research needed. SOAP 1.1/1.2 specs are stable and fully documented.

### Phase 2: XSD Schema Parser

**Rationale:** XSD parsing is the deepest dependency in the graph — WSDL parsing cannot produce a validated ServiceModel without it. The two-pass resolve pattern must be proven on XSD (the harder grammar) before WSDL parsing begins. Unblocking XSD first reduces integration risk.

**Delivers:** A TypeRegistry keyed by QName, covering complexType, simpleType, element, attribute, extension, restriction, and cross-file imports.

**Addresses:** XSD schema parsing (table stakes), XSD forward reference resolution

**Avoids:** Incomplete extension/restriction chain resolution (Pitfall 2); silent import skip on missing schema files (Pitfall 6)

**Research flag:** May benefit from phase research to validate the two-pass resolve algorithm against ONVIF's multi-file XSD set before committing to the data model. python-zeep's implementation is the reference.

### Phase 3: WSDL Parser and Service Model

**Rationale:** With XSD parsing complete, WSDL parsing follows naturally. This phase produces the ServiceModel and the dispatch table key set (body element QNames per operation). It is also the phase where import resolution correctness must be proven against multi-file ONVIF WSDLs.

**Delivers:** A validated ServiceModel assembled from WSDL definitions + XSD TypeRegistry; dispatch table keyed on body element QNames; WSDL bytes for serving.

**Addresses:** WSDL 1.1 parsing, document/literal dispatch table construction, WSDL serving on GET `?wsdl` with address rewriting

**Avoids:** Single-pass WSDL parsing (design choice — two-pass is mandatory); WSDL import resolution silent skip (Pitfall 6); WSDL address rewriting broken under reverse proxy (Pitfall 9); roxmltree DOCTYPE rejection (Pitfall 10)

**Research flag:** Standard patterns — node-soap, python-zeep, and WCF all document the dispatch-by-body-QName pattern. Address rewriting is well-understood.

### Phase 4: Dispatch and Router Integration

**Rationale:** With the ServiceModel and handler trait in place, dispatch and axum wiring can be assembled. This phase produces the first end-to-end request path — a real SOAP request can be handled and a response returned. It is also the integration point where axum-specific gotchas (body size limits, content-type routing absence) must be addressed.

**Delivers:** A working `SoapRouter::into_router()` that accepts POST requests, dispatches by body element QName to a raw `SoapHandler`, and serves WSDL on GET `?wsdl`.

**Addresses:** Document/literal dispatch, axum Router integration, raw handler trait, operation routing

**Avoids:** Dispatching on SOAPAction alone (Pitfall 5); axum body size limit silent truncation; axum content-type routing absence

**Research flag:** Standard patterns — axum Router composition is well-documented. No additional research needed.

### Phase 5: WS-Security UsernameToken

**Rationale:** Once the dispatch path works end-to-end, security can be layered on as a pre-dispatch interceptor without restructuring the rest of the pipeline. This matches how all production implementations (CXF, WCF, node-soap) structure it. The per-operation bypass (GetSystemDateAndTime) is handled here.

**Delivers:** PasswordDigest and PasswordText validation, timestamp freshness enforcement (5-minute window), nonce replay prevention via rotating-bucket cache, per-operation auth bypass list.

**Addresses:** WS-Security UsernameToken, per-operation auth bypass, nonce replay protection, mustUnderstand fault generation for unprocessed security headers

**Avoids:** PasswordDigest byte order error (Pitfall 8 — verified with known test vector before implementation); nonce cache unbounded growth (Pitfall 4 — rotating bucket from day one); auth bypass checked before WS-Security parse (security timing note from PITFALLS.md)

**Research flag:** Standard patterns for PasswordDigest computation (spec is unambiguous). Nonce cache design has a well-documented rotating-bucket pattern from Apache CXF. No additional research needed.

### Phase 6: SOAP 1.1 Backfill and Broader Compatibility

**Rationale:** After v1 validation with an ONVIF consumer, SOAP 1.1 support and multi-service WSDL handling are the highest-value additions for non-ONVIF use cases. These are additive — they extend existing code paths without restructuring them.

**Delivers:** SOAP 1.1 envelope parsing, SOAP 1.1 fault format, multiple services per WSDL, RPC/encoded dispatch as fallback.

**Addresses:** SOAP 1.1 envelope + fault (v1.x), multiple services per WSDL (v1.x), RPC/encoded binding style (v1.x)

**Avoids:** SOAP version namespace confusion — the SoapVersion enum from Phase 1 is already in place; this phase exercises both code paths

**Research flag:** Standard patterns — SOAP 1.1 spec is stable. RPC/encoded dispatch is well-understood. No additional research needed.

### Phase 7: MTOM/XOP and Advanced Features

**Rationale:** MTOM requires a distinct code path (multipart MIME, not pure XML) and conflicts with the streaming XML parse for request bodies. It is correctly deferred until the core is proven stable, as its detection must happen before the SOAP envelope parser is invoked.

**Delivers:** MTOM/XOP multipart MIME parsing, XOP Include element replacement, payload validation against XSD.

**Addresses:** MTOM/XOP support (v1.x), payload validation against XSD (v1.x)

**Research flag:** MTOM integration needs phase research — the interaction between multipart MIME detection and the quick-xml streaming path is non-trivial. The Content-Type routing change affects the axum handler entry point.

### Phase Ordering Rationale

- The feature dependency graph from FEATURES.md defines a hard order: XSD → WSDL → ServiceModel → dispatch → WS-Security. No phase can be safely reordered.
- Fault generation and envelope parsing are in Phase 1 because they are consumed by every subsequent phase in error paths. Building them last would mean all prior phases have untestable error handling.
- WS-Security is in Phase 5 (not earlier) because it is a pre-dispatch interceptor — it needs the dispatch infrastructure to know which operations require auth. Adding it before Phase 4 creates circular dependencies.
- SOAP 1.1 is deferred to Phase 6 because ONVIF (the v1 validation target) mandates SOAP 1.2. Proving the design on one version before adding the second reduces integration risk.
- MTOM is last because its Content-Type detection hook sits above the SOAP layer — adding it requires a controlled modification to the router entry point, which should be stable before that change is made.

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 2 (XSD Parser):** The two-pass resolve algorithm for extension/restriction inheritance chains across multiple schema files is the most complex piece in the codebase. Recommend reviewing python-zeep's `xsd/elements/complex.py` and `xsd/types/complex.py` in detail before designing the Rust data model. A wrong data model here causes rework in all downstream phases.
- **Phase 7 (MTOM):** MTOM/XOP multipart MIME detection and integration with the quick-xml streaming path needs dedicated research. The axum body handling changes required are non-trivial.

Phases with standard patterns (skip research-phase):
- **Phase 1 (Foundation):** SOAP 1.1/1.2 envelope structure and fault formats are fully specified in W3C specs. No ambiguity.
- **Phase 3 (WSDL Parser):** python-zeep and node-soap both document the WSDL parsing model in detail. Two-pass resolve pattern is proven.
- **Phase 4 (Dispatch + Router):** axum Router composition and body extraction patterns are well-documented. Dispatch by body QName is documented in WCF and node-soap.
- **Phase 5 (WS-Security):** PasswordDigest formula is unambiguous in the OASIS spec. Nonce cache rotating-bucket design is documented in Apache CXF.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | All versions verified against crates.io API; library characteristics confirmed against official READMEs and docs.rs |
| Features | HIGH | SOAP spec is stable; feature expectations drawn from Apache CXF, Spring-WS, node-soap, PHP SoapServer — all production implementations with well-documented behavior |
| Architecture | HIGH | WSDL/XSD two-pass pattern verified against python-zeep source; dispatch-by-body-QName verified in node-soap and WCF; WS-Security formula verified against OASIS spec |
| Pitfalls | HIGH (spec) / MEDIUM (Rust-specific) | Core SOAP/WS-Security spec pitfalls are confirmed in multiple production sources; Rust-specific implementation patterns are inferred from general async Rust best practices |

**Overall confidence:** HIGH

### Gaps to Address

- **Typed handler API approach:** Whether to use yaserde, quick-xml serde-style, or a custom XSD-to-Rust-type mapping for a future typed handler layer is unresolved. The raw handler trait unblocks everything for v1; this is a v2 design question. Flag for a research spike when the raw handler is validated.
- **Nonce cache persistence across restarts:** The in-process rotating-bucket design is correct for single-instance deployments. Consumers needing replay protection across crashes or horizontal scale must provide an external store (Redis, etc.). The API should expose a `NonceCacheBackend` trait from day one, even if only the in-memory implementation ships in v1.
- **WSDL import over HTTP:** The PITFALLS.md notes that allowing WSDL imports from arbitrary URLs introduces SSRF risk. The `SchemaLoader` trait approach (consumer provides the loader) sidesteps this, but the default behavior and documentation need a deliberate decision during Phase 3 planning.

## Sources

### Primary (HIGH confidence)
- crates.io API — version verification for all 12 crates in the recommended stack
- [OASIS WS-Security UsernameToken Profile 1.1.1](https://docs.oasis-open.org/wss-m/wss/v1.1.1/os/wss-UsernameTokenProfile-v1.1.1-os.html) — PasswordDigest formula, nonce requirements, 5-minute timestamp window
- [W3C SOAP 1.2 Messaging Framework](https://www.w3.org/TR/soap12-part1/) — mustUnderstand, fault structure, envelope namespace
- [ONVIF Core Specification](https://www.onvif.org/specs/core/ONVIF-Core-Specification.pdf) — SOAP 1.2 document/literal mandate, WS-Security requirements, GetSystemDateAndTime auth bypass pattern
- [python-zeep WSDL internals](https://docs.python-zeep.org/en/master/internals_wsdl.html) — two-pass parse architecture, module structure reference
- [node-soap server.ts](https://github.com/vpulim/node-soap/blob/master/src/server.ts) — dispatch by body first-child QName algorithm
- [axum docs.rs](https://docs.rs/axum/latest/axum/) — Router composition and body extraction patterns
- [roxmltree GitHub](https://github.com/RazrFalcon/roxmltree) — DOM vs streaming tradeoff; DOCTYPE limitation (issue #56)

### Secondary (MEDIUM confidence)
- [Spring-WS server reference](https://docs.spring.io/spring-ws/sites/2.0/reference/html/server.html) — interceptor chain, endpoint mapping, WSDL handling
- [Apache CXF nonce caching](https://coheigea.blogspot.com/2012/04/security-token-caching-in-apache-cxf-26.html) — rotating-bucket nonce cache design
- [python-zeep changelog](https://docs.python-zeep.org/en/master/changes.html) — historical xsd:extension/restriction refactors confirming inheritance chain gap is a common pitfall
- [WSDL binding styles — IBM](https://developer.ibm.com/articles/ws-whichwsdl/) — document/literal vs RPC/encoded compliance
- [Dispatch by Body Element — Microsoft WCF](https://learn.microsoft.com/en-us/dotnet/framework/wcf/samples/dispatch-by-body-element) — document/literal dispatch pattern

### Tertiary (LOW confidence)
- WebSearch: Rust SOAP server ecosystem 2025 — confirmed no production server crate exists (absence of results is the finding)

---
*Research completed: 2026-04-03*
*Ready for roadmap: yes*
