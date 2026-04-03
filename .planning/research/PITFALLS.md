# Pitfalls Research

**Domain:** Rust SOAP server crate — WSDL/XSD parsing, SOAP 1.1/1.2, WS-Security
**Researched:** 2026-04-03
**Confidence:** HIGH (core SOAP/WS-Security spec pitfalls) / MEDIUM (Rust-specific, implementation patterns)

## Critical Pitfalls

### Pitfall 1: Namespace Inheritance Loss When Extracting XML Fragments

**What goes wrong:**
Namespace prefixes declared on the SOAP envelope root element (e.g., `xmlns:xsd="..."`) are visible to child nodes in the DOM, but when you extract the Body element as a substring or feed it to a secondary parser, those ancestor namespace declarations disappear. The secondary parser sees bare prefixes with no binding and throws "prefix not bound" errors — even though the original document was valid.

**Why it happens:**
Developers treat XML fragment extraction as a simple substring operation. SOAP bodies are routinely passed to downstream handlers as raw bytes. If the handler receives only the `<Body>` subtree without its inherited namespace context, any element or attribute in that subtree using a prefix declared on `<Envelope>` becomes unresolvable.

**How to avoid:**
- Never pass raw XML substrings between parser phases. Pass the parsed DOM node with its namespace context intact, or re-emit the fragment with all in-scope namespace declarations written explicitly on the root element.
- With quick-xml streaming, collect all namespace mappings from `NsReader` at every depth level and inject them when re-serializing a sub-document.
- Establish a rule: every public API boundary that crosses an XML fragment accepts `(bytes, namespace_map)` not bare bytes.

**Warning signs:**
- "prefix X is not bound" errors that only appear on specific messages
- Tests pass with simple SOAP envelopes but fail with real-world ONVIF messages that declare namespaces high up
- Namespace errors disappear when you copy declarations onto the inner element manually

**Phase to address:**
SOAP envelope parsing phase — before any dispatch or handler code.

---

### Pitfall 2: XSD extension/restriction Not Fully Walking the Inheritance Chain

**What goes wrong:**
When resolving a `complexType` that uses `xsd:extension` or `xsd:restriction`, only the declared type's own elements are collected. The base type's elements are ignored or partially merged. Resulting dispatch maps are missing fields, or validation against the schema fails on real messages that use derived types.

**Why it happens:**
Extension and restriction require recursively ascending the inheritance chain, potentially across multiple schema files and namespaces. Developers implement the common single-level case and ship it, only discovering that ONVIF WSDLs use multi-level inheritance (e.g., PTZNode extends DeviceEntity which has its own elements).

python-zeep's changelog explicitly documents "major refactor of xsd:extension / xsd:restriction" as a post-ship correction, confirming this is a historically common gap.

**How to avoid:**
- In the two-pass parse: during resolution, fully flatten inheritance chains before emitting the type descriptor. Type `T extends B` must produce a descriptor containing B's elements prepended to T's own elements.
- Write test fixtures with 3-level inheritance chains before implementing extension/restriction.
- Port zeep's `xsd/elements/complex.py` resolution logic precisely rather than re-implementing from scratch.

**Warning signs:**
- ONVIF GetCapabilities or GetSystemDateAndTime responses parse correctly but PTZ operations fail
- Types with no direct elements parse fine; types that only add fields via extension fail
- Schema validation says an element is unexpected when the element is defined on a base type

**Phase to address:**
XSD schema parser phase — specifically the resolve pass.

---

### Pitfall 3: SOAP 1.1 vs 1.2 Envelope Namespace Confusion in Dispatch

**What goes wrong:**
SOAP 1.1 uses namespace `http://schemas.xmlsoap.org/soap/envelope/` with Content-Type `text/xml`. SOAP 1.2 uses `http://www.w3.org/2003/05/soap-envelope` with Content-Type `application/soap+xml`. If the server hard-codes one namespace string for `Envelope`, `Body`, `Header`, `Fault` element matching, messages from the other version are silently mis-dispatched or cause "element not found" panics.

**Why it happens:**
Developers prototype against one version (typically 1.2 for ONVIF) and never test the other. The content-type and namespace are distinct signals that must both be checked. SOAP 1.2 also renames `actor` to `role` and changes `SOAPAction` to be a media type parameter — these are silent compatibility traps.

**How to avoid:**
- Define an enum `SoapVersion { V11, V12 }` detected from the `Content-Type` header before XML parsing.
- Use version-specific namespace constants for all element matching — never bare string literals in dispatch code.
- Test with both versions from day one using a matrix test fixture.
- For fault generation, gate the fault namespace, fault code structure, and HTTP status code (SOAP 1.2 returns 400/500, not always 500) on the detected version.

**Warning signs:**
- ONVIF clients that send 1.1 requests receive malformed faults or empty responses
- SOAPAction header is present but ignored (required for dispatch in 1.1, optional in 1.2)
- Fault test suite only covers one version

**Phase to address:**
SOAP envelope parsing phase — version detection must precede all other parsing.

---

### Pitfall 4: WS-Security Nonce Cache Not Bounded — Resource Exhaustion / Replay Attack

**What goes wrong:**
Either the nonce cache grows unboundedly (memory leak under load) or the cache is cleared too aggressively (replays accepted). If nonce entries are never evicted, a long-running server accumulates all nonces ever received. If the cache is not persisted across restarts, a replay within the freshness window (typically 5 minutes) will succeed after a crash-restart.

**Why it happens:**
The WS-Security spec says "cache used nonces for at least the freshness window." Developers implement a `HashSet<String>` with no expiry, not realizing the cache must be time-bounded. The interaction between nonce TTL, timestamp freshness validation, and clock skew tolerance requires careful design.

**How to avoid:**
- Use a time-bucketed nonce store: two `HashSet`s rotated every T/2 seconds where T is the freshness window (e.g., 5 minutes). A nonce is valid if absent from both sets; after check, insert into the active set. Rotate: drop old set, new set becomes old, create fresh new set.
- Enforce timestamp Created within ±5 minutes (allow configurable clock skew up to 60 seconds).
- Log replayed nonce attempts at WARN level — they indicate either attacks or misbehaving clients.
- Document that the nonce cache is in-process only — consumers who need replay protection across restarts must provide a shared store.

**Warning signs:**
- Memory grows continuously on devices with frequent ONVIF polling
- A replayed SOAP message is accepted after server restart
- Two concurrent requests with identical nonces both succeed

**Phase to address:**
WS-Security phase.

---

### Pitfall 5: Document/Literal Dispatch Requires Body Element QName, Not SOAPAction Alone

**What goes wrong:**
The server dispatches based on the `SOAPAction` HTTP header and misses requests where the client omits it or sends a different value. Conversely, the server ignores the body element QName and only uses SOAPAction, which is unreliable in document/literal style because it is optional/advisory.

**Why it happens:**
RPC-style thinking: "the action tells you what method to call." In document/literal, the spec says the body element's qualified name is the definitive dispatch key. SOAPAction is advisory and many document/literal clients send an empty string `""` or omit it entirely. ONVIF clients behave this way.

**How to avoid:**
- Primary dispatch key: QName of the first child element of `<Body>` (`{namespace}localName`).
- SOAPAction as secondary fallback only when body element QName is ambiguous or absent.
- In the WSDL parser, build the dispatch table keyed on body element QName extracted from `wsdl:input` → `wsdl:part` → `element` reference.
- Document this design decision prominently in the crate's API docs.

**Warning signs:**
- Operations work when tested with SoapUI (which sends SOAPAction) but fail with real camera clients
- Empty SOAPAction causes a "method not found" fault
- Two operations that happen to share a SOAPAction but differ in body element are not distinguishable

**Phase to address:**
WSDL parser phase (build correct dispatch table) and SOAP dispatch phase (use QName).

---

### Pitfall 6: WSDL Import/Include Resolution Silently Skips Unreachable Schemas

**What goes wrong:**
A WSDL references XSD schemas via `xs:import schemaLocation="..."`. If the schema file is unavailable at parse time (wrong relative path, embedded vs external mismatch), the import is silently skipped. Types from that schema are unresolvable at runtime, causing dispatch failures on any operation using those types — but only discovered at request time, not startup.

**Why it happens:**
Parsers often treat import failures as non-fatal warnings. Developers test with a single embedded WSDL and never exercise the multi-file case. The WSDL 1.1 spec allows a WSDL to import other WSDLs (`wsdl:import`) and inline XSD schemas can import external XSD files (`xs:import`), creating a tree that must be fully resolved.

**How to avoid:**
- Fail fast at startup: treat any unresolved `xs:import` or `wsdl:import` as a hard error, not a warning.
- Implement a schema registry that tracks which namespaces have been resolved; after parsing, assert no forward references remain.
- Provide a `SchemaLoader` trait that callers can implement to supply inline schemas, file paths, or HTTP fetching — don't hard-code file I/O.
- Test with multi-file WSDL sets (ONVIF uses `onvif.wsdl` + `common.xsd` + `analytics.xsd` etc.).

**Warning signs:**
- Startup succeeds but specific operations fail with "type unknown" at runtime
- Schema import errors are logged as warnings rather than errors
- Tests only use single-file WSDLs

**Phase to address:**
WSDL parser phase — schema loading and import resolution.

---

### Pitfall 7: mustUnderstand Header Not Generating the Correct Fault Structure

**What goes wrong:**
A SOAP header with `mustUnderstand="1"` targeting the current node arrives, but the server either ignores it (security risk), processes it silently (may be wrong), or generates a malformed fault. In SOAP 1.2, the fault must set `env:Code/env:Value` to `env:MustUnderstand` and MUST stop further processing. In SOAP 1.1, the fault code must be `soap:MustUnderstand`.

**Why it happens:**
WS-Security headers carry `mustUnderstand="1"`. Developers implement happy-path header processing but skip the "what if I don't understand this header" case. The fault structure for `MustUnderstand` violations is different from application faults — it lists the unrecognized header QNames.

**How to avoid:**
- Build a header processing pipeline that explicitly tracks which headers have been "understood." After all processors run, any unprocessed `mustUnderstand="1"` header targeted at the current role must produce a `MustUnderstand` fault before calling the operation handler.
- Generate version-correct fault envelopes: SOAP 1.1 uses `<faultcode>`, `<faultstring>`; SOAP 1.2 uses `<env:Code>`, `<env:Reason>`, `<env:Detail>`.
- Include the offending header QName(s) in the fault detail.

**Warning signs:**
- WS-Security errors produce generic `Server` faults instead of `MustUnderstand` faults
- Unrecognized extension headers pass through silently without fault
- SOAP 1.1 clients receive 1.2-structured faults

**Phase to address:**
SOAP envelope parsing phase (header processing pipeline) and SOAP fault generation phase.

---

### Pitfall 8: PasswordDigest Byte Order — Nonce Must Be Decoded Before Concatenation

**What goes wrong:**
The PasswordDigest computation is `SHA1(B64DECODE(Nonce) || Created || Password)`. If the raw Base64 string is concatenated instead of its decoded bytes, the digest never matches the client's value. This produces an authentication failure that is indistinguishable from a wrong password.

**Why it happens:**
The OASIS WS-UsernameToken spec states the nonce bytes must be decoded from Base64 before hashing. It is easy to miss this and concatenate the Base64 string directly, since the `Nonce` element value in the XML is Base64-encoded.

**How to avoid:**
- Write the exact digest computation as a unit test with a known nonce/timestamp/password/digest tuple taken from the rpos reference implementation before implementing the logic.
- Annotate the code with the formula: `digest = base64(sha1(b64decode(nonce) ++ created_utf8_bytes ++ password_utf8_bytes))`.
- Test against at least one real ONVIF camera response or the ONVIF conformance test tool.

**Warning signs:**
- All PasswordDigest authentications fail; PasswordText authentication succeeds
- Digest authentication fails even with the exact credentials from a working reference client
- No unit test exists with a hard-coded expected digest value

**Phase to address:**
WS-Security phase — UsernameToken authentication.

---

### Pitfall 9: WSDL Address Rewriting Breaks Under Reverse Proxy / TLS Termination

**What goes wrong:**
When serving the WSDL on GET `?wsdl`, the `soap:address location` must be rewritten to the address the client actually reached. If the server reads its own `HOST` header or local bind address, but is behind a reverse proxy, the rewritten address points to the internal address (e.g., `http://127.0.0.1:3000`) instead of the public URL. WSDL clients then try to connect to an unreachable address.

**Why it happens:**
The server sees the internal socket address, not the public URL. Without explicit configuration for `external_base_url`, the rewriter has no way to know the public address.

**How to avoid:**
- Expose a `base_url: Option<String>` configuration field on the server builder. If set, use it verbatim for `soap:address`. If not set, derive from the `Host` request header as a fallback.
- Default to using the `Host` header (which reverse proxies typically preserve), not the local socket address.
- Document the `X-Forwarded-Proto` / `X-Forwarded-Host` concern and recommend consumers set `base_url` explicitly in production.

**Warning signs:**
- WSDL is served correctly but ONVIF clients cannot connect after downloading it
- `soap:address` in served WSDL shows `127.0.0.1` or `localhost`
- Works in development, breaks behind nginx/caddy

**Phase to address:**
WSDL serving phase.

---

### Pitfall 10: roxmltree DOCTYPE Parsing Failure on Unusual WSDL Files

**What goes wrong:**
roxmltree rejects XML documents containing `DOCTYPE` declarations (known issue, GitHub issue #56). Some WSDLs from enterprise systems include DTD references. Parsing fails with a hard error at startup, providing no useful diagnostic.

**Why it happens:**
roxmltree is designed for simplicity and does not implement DTD processing. This is a documented limitation. Since ONVIF WSDLs don't use DOCTYPE, this is easy to miss in testing.

**How to avoid:**
- Add DOCTYPE stripping as a preprocessing step before passing bytes to roxmltree — strip the `<!DOCTYPE ...>` declaration if present (it is not needed for WSDL validation).
- Emit a structured warning when DOCTYPE stripping occurs so consumers know their WSDL contained a non-standard declaration.
- Alternatively, document this limitation clearly in the crate README.

**Warning signs:**
- "Unexpected token" or "DOCTYPE not supported" error at startup with non-ONVIF WSDLs
- No error message indicating which file or location caused the failure

**Phase to address:**
WSDL parser phase — input preprocessing.

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Single-pass WSDL parse (no resolve phase) | Simpler code, faster initial implementation | Forward references fail; ONVIF multi-file WSDLs break silently | Never — two-pass is required for correctness |
| Hard-coding SOAP 1.2 namespace strings | Faster prototype | SOAP 1.1 support requires finding and replacing all strings | Never — use constants from day one |
| `HashSet<String>` with no expiry for nonce cache | Zero-config replay detection | Memory leak on long-running servers | MVP only if documenting clearly that production use requires proper implementation |
| Treating `SOAPAction` as primary dispatch key | Simpler dispatch logic | Breaks with ONVIF and any document/literal client that omits SOAPAction | Never — body QName must be primary |
| Skipping `mustUnderstand` header enforcement | Faster to ship happy path | Security bypass: WS-Security headers silently ignored | Never for any handler that processes security headers |
| Returning raw Rust panic messages in fault detail | Easy debugging | Leaks internal implementation details to callers | Never in release builds |
| Assuming schemas are embedded in WSDL | Avoids file loading complexity | Multi-file WSDLs (ONVIF) cannot be loaded | Never — must support external schemas |

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| axum Router composition | Registering SOAP handler on POST only, forgetting GET for `?wsdl` | Register both: `POST /service` for SOAP, `GET /service?wsdl` via query parameter extraction |
| axum content-type routing | Relying on axum's built-in content-type routing (it doesn't exist) | Check `Content-Type` header manually in the handler; axum routes by path only |
| axum body size limits | Default body limit (2MB) silently truncates large SOAP messages with attachments | Set `DefaultBodyLimit::disable()` or configure an explicit higher limit for the SOAP route |
| quick-xml NsReader | Calling `reader.read_event()` instead of `reader.read_resolved_event()` — misses namespace resolution | Always use `read_resolved_event()` when namespace-aware parsing is required |
| roxmltree namespace lookup | Using `node.tag_name().name()` without checking namespace — matches elements from any namespace | Always match on `node.tag_name().namespace()` AND `node.tag_name().name()` together |
| ONVIF cameras | Sending `SOAPAction` header and expecting it to route correctly | ONVIF uses document/literal; dispatch on body element QName, not SOAPAction |

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Deserializing full WSDL/XSD on every request | Latency spikes, high CPU | Parse WSDL at startup, store dispatch table as `Arc<DispatchTable>` shared across threads | First request, or first concurrent request |
| Cloning namespace maps per XML event | High allocation rate in quick-xml hot path | Pre-build a namespace context map; clone only when entering a new scope | Under load with complex SOAP envelopes |
| Nonce cache using `Mutex<HashMap>` under high ONVIF poll rate | Lock contention, latency spikes | Use the rotating-bucket design with `RwLock` or a lock-free structure | ~10+ concurrent ONVIF clients |
| Allocating `String` for every XML attribute value | High GC pressure | Use `Cow<str>` or borrowed slices where attribute values are short-lived | Under sustained load |
| Re-parsing SOAP fault template XML on every error | CPU overhead on error paths | Pre-build fault envelope bytes at startup, substitute values via string formatting or template | Error-heavy scenarios |

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| Accepting PasswordText credentials without TLS | Password transmitted in plaintext | Document that PasswordText requires TLS; optionally reject PasswordText in non-TLS context (check via forwarded headers) |
| Not validating timestamp Created field freshness | Replay attacks — any captured token valid indefinitely | Reject tokens where `Created` is older than 5 minutes or in the future by more than 60 seconds |
| Nonce uniqueness check using only the nonce, not per-user | Nonce collision across users is still a replay | Cache nonces globally (not per-user) as the spec intends — a nonce value MUST be globally unique |
| Auth bypass list checked after full WS-Security processing | Timing side-channel on bypassed operations | Check bypass list before attempting WS-Security parse, not after auth failure |
| Including full stack trace in SOAP fault `<Detail>` | Information leakage for attackers | Strip internal details; log them server-side only |
| Accepting WSDL imports from arbitrary URLs at startup | SSRF if WSDL is user-supplied | Restrict schema loading to local filesystem by default; require explicit opt-in for HTTP imports |

## "Looks Done But Isn't" Checklist

- [ ] **SOAP 1.1 support:** Happy-path messages work, but check fault structure uses `<faultcode>/<faultstring>/<detail>` not SOAP 1.2 structure — verify with a SOAP 1.1 fault test
- [ ] **XSD extension resolution:** Simple types pass, but verify a type with 3-level inheritance chain produces all elements from all ancestor types
- [ ] **Nonce replay detection:** Authentication works, but send the same nonce twice within 5 minutes — second request must be rejected
- [ ] **Timestamp validation:** Authentication works with fresh tokens, but send a token with `Created` 10 minutes ago — must be rejected
- [ ] **WSDL address rewriting:** WSDL is served, but confirm the `soap:address location` reflects the request's Host header, not the internal bind address
- [ ] **mustUnderstand enforcement:** Operations dispatch correctly, but send a header with an unknown QName and `mustUnderstand="1"` — must get a `MustUnderstand` fault, not a `Server` fault or silent success
- [ ] **Multi-file WSDL:** Single-file WSDLs load correctly, but test with ONVIF's multi-file set including imports — all types must resolve
- [ ] **Body QName dispatch:** Dispatching works with SOAPAction present, but send the same request with SOAPAction empty string — must still dispatch correctly
- [ ] **Namespace context in handlers:** Handler receives body bytes that parse correctly, but confirm parsed bytes contain all ancestor namespace declarations needed to resolve any prefix used in the body
- [ ] **DOCTYPE in WSDL:** Standard WSDLs load, but test that a WSDL with a `<!DOCTYPE>` declaration either loads with a warning or produces a clear error, not a panic

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Namespace fragment loss discovered post-ship | HIGH | Redesign handler API to pass namespace context alongside bytes; update all callers |
| XSD extension chain incomplete | MEDIUM | Add recursive resolution pass to the resolve phase; existing type descriptors can be rebuilt at startup |
| Wrong dispatch key (SOAPAction vs body QName) | HIGH | Dispatch table rebuild; all registered operations must re-register with QName keys; breaking API change for handler registration |
| Nonce cache unbounded | LOW | Replace `HashSet` with rotating-bucket; no API change needed |
| SOAP version namespace hard-coded | MEDIUM | Extract constants; find all comparison sites; regression test both versions |
| PasswordDigest byte order wrong | LOW | Fix single computation; verify with known test vector; no structural change |
| WSDL address rewriting ignores Host header | LOW | Add `base_url` config field; default to Host header; one-line fix in WSDL serving handler |

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| Namespace inheritance loss in fragments | SOAP envelope parsing | Test: extract body bytes, re-parse with secondary parser, confirm all prefixes resolve |
| XSD extension chain incomplete | XSD schema parser (resolve pass) | Test: 3-level inheritance fixture produces all ancestor elements |
| SOAP 1.1 vs 1.2 version confusion | SOAP envelope parsing | Matrix test: same operation via 1.1 and 1.2 both succeed with version-correct responses |
| WS-Security nonce cache unbounded | WS-Security phase | Load test: 1000 unique authentications; measure memory before and after |
| Wrong dispatch key | WSDL parser + dispatch phase | Test: empty SOAPAction + body QName dispatches correctly |
| WSDL import resolution silent skip | WSDL parser phase | Test: multi-file WSDL with missing import fails hard at startup |
| mustUnderstand not enforced | SOAP envelope parsing (header pipeline) | Test: unknown mustUnderstand header produces MustUnderstand fault |
| PasswordDigest byte order | WS-Security phase | Unit test: known nonce/created/password/digest tuple from rpos reference |
| WSDL address rewriting broken | WSDL serving phase | Integration test: request with `Host: example.com` produces WSDL with `soap:address location="http://example.com/..."` |
| DOCTYPE rejection | WSDL parser phase | Test: WSDL with DOCTYPE either loads with warning or emits clear error |
| Body QName dispatch not primary | Dispatch phase | Test: document/literal request with empty SOAPAction dispatches on body QName |
| Fault structure version mismatch | Fault generation phase | Test: SOAP 1.1 client receives 1.1-structured fault; 1.2 client receives 1.2-structured fault |

## Sources

- [OASIS WS-Security UsernameToken Profile 1.1.1](https://docs.oasis-open.org/wss-m/wss/v1.1.1/os/wss-UsernameTokenProfile-v1.1.1-os.html) — nonce and timestamp requirements
- [W3C SOAP 1.2 Messaging Framework](https://www.w3.org/TR/soap12-part1/) — mustUnderstand, fault structure, role/actor
- [W3C SOAP 1.1 vs 1.2 migration guide](https://www.w3.org/2003/06/soap11-soap12) — namespace and content-type differences
- [python-zeep changelog](https://docs.python-zeep.org/en/master/changes.html) — historical record of xsd:extension/restriction refactors confirming this is a common pitfall
- [python-zeep WSDL internals](https://docs.python-zeep.org/en/master/internals_wsdl.html) — two-pass parse architecture reference
- [roxmltree GitHub issue #56](https://github.com/RazrFalcon/roxmltree/issues/56) — DOCTYPE parsing limitation
- [ONVIF WS-Security PasswordDigest formula](https://github.com/onvif/specs/discussions/163) — canonical nonce decode before hash
- [ONVIF authentication reference](https://docs.edgexfoundry.org/3.0/microservices/device/supported/device-onvif-camera/supplementary-info/onvif-user-authentication/) — replay protection requirements
- [Apache CXF nonce caching](https://coheigea.blogspot.com/2012/04/security-token-caching-in-apache-cxf-26.html) — nonce cache design patterns
- [WSO2 mustUnderstand explanation](https://wso2.com/library/tutorials/understand-famous-did-not-understand-mustunderstand-header-s-error/) — fault generation for unrecognized headers
- [SOAP document/literal dispatch](https://johnragan.wordpress.com/2010/01/04/soap-messages-rpc-vs-document-vs-literal-vs-encoded-vs-wrapped-vs-unwrapped/) — body QName as primary dispatch key
- [axum content-type routing limitation](https://github.com/tokio-rs/axum/issues/1654) — no built-in content-type routing
- [WSDL import resolution — Apache Tuscany](https://cwiki.apache.org/confluence/display/TUSCANYWIKI/Resolving+WSDL+and+XSD+artifacts) — multi-file schema loading patterns
- [WS-Security nonce reuse — CoreWCF discussion](https://github.com/CoreWCF/CoreWCF/discussions/1400) — implementation details

---
*Pitfalls research for: Rust SOAP server crate (WSDL/XSD parsing, SOAP 1.1/1.2, WS-Security)*
*Researched: 2026-04-03*
