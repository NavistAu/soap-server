# Phase 1: ONVIF-Level Support - Research

**Researched:** 2026-04-04
**Domain:** Rust SOAP server — WSDL/XSD parsing, SOAP 1.2 dispatch, WS-Security UsernameToken, axum Router integration
**Confidence:** HIGH

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

- **Porting approach:** This is a porting exercise — match the spec and follow the best available prior art
- **Primary WSDL/XSD source:** python-zeep (two-pass pattern)
- **Dispatch reference:** node-soap (body element QName → handler)
- **WS-Security reference:** rpos (cleanest 50-line UsernameToken digest implementation)
- **Supplementary dispatch reference:** globusdigital/soap (Go) for minimal dispatch pattern
- **Architecture authority:** DESIGN.md is the authoritative specification
- **Dependencies locked:** roxmltree (startup DOM), quick-xml (per-request streaming), axum + tokio, sha1 + base64 + chrono
- **License:** MIT OR Apache-2.0
- **Crate structure:** lib.rs, server.rs, dispatch.rs, envelope.rs, fault.rs, security.rs, wsdl/, xsd/

### Claude's Discretion

- **Handler boundary / namespace context:** How to handle namespace inheritance loss when body bytes are extracted for the raw handler. Options include re-emitting namespace declarations on the fragment root or passing a (bytes, namespace_map) tuple.
- **Auth model:** Whether the auth layer takes a single static credential or a lookup function for multi-user support.
- **WSDL loading API:** Whether to support from_file() only or also from_bytes/from_str for embedded WSDLs.
- **Test fixtures:** Which ONVIF WSDLs to use, whether to bundle in repo or download at test time.

### Deferred Ideas (OUT OF SCOPE)

None — discussion stayed within phase scope.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| XSD-01 | Parser reads XSD schemas and constructs in-memory type graph (elements, complexType, simpleType, attributes, groups) | python-zeep xsd/visitor.py provides complete visit_* method list; roxmltree DOM traversal pattern established |
| XSD-02 | Supports xs:sequence, xs:all, xs:choice content models | python-zeep SchemaVisitor has visit_sequence, visit_all, visit_choice methods with identical signature pattern |
| XSD-03 | Supports xs:extension and xs:restriction (type inheritance) with recursive chain resolution | zeep xsd/types/complex.py: resolve() calls self._extension.resolve() recursively before extend(); memoized with _resolved flag |
| XSD-04 | Supports xs:import and xs:include with cycle detection and caching | zeep wsdl.py transport-abstracted loader; import guard: "if self._resolved_imports: return" pattern |
| XSD-05 | Supports xs:element with ref, minOccurs, maxOccurs, nillable, default, fixed | XsdElement struct in DESIGN.md captures all attributes; ref_attr field handles element ref= |
| XSD-06 | Supports xs:attribute and xs:attributeGroup with ref, use, default, fixed | XsdAttribute struct in DESIGN.md; visit_attribute and visit_attributeGroup in zeep visitor |
| XSD-07 | Supports xs:group for reusable content groups | zeep visitor has visit_group; ONVIF onvif.xsd uses xs:group |
| XSD-08 | Supports xs:any and xs:anyAttribute extensibility points | ONVIF uses xs:any (namespace="##any", processContents="lax") and xs:anyAttribute on most types |
| XSD-09 | Supports xs:simpleType restrictions (enumeration, minInclusive, maxInclusive, pattern, length, etc.) | Restriction struct in DESIGN.md has all facets; ONVIF uses enumeration extensively (VideoEncoding, H264Profile, etc.) |
| XSD-10 | Supports xs:list and xs:union compound simple types | ONVIF uses xs:list (IntAttrList, FloatAttrList, StringAttrList, ReferenceTokenList); zeep has visit_list, visit_union |
| XSD-11 | Payload validation — validate request body XML against the operation's input XSD schema before handler invocation | Requires TypeRegistry lookup by QName after XSD-01-10 are complete; validation pass walks resolved type graph |
| WSDL-01 | Parser reads WSDL 1.1 XML and constructs in-memory representation (services, port types, bindings, messages, operations) | zeep parse.py has 5 pure functions; WsdlDefinition struct in DESIGN.md matches zeep's Definition class |
| WSDL-02 | Two-pass resolution — parse pass collects raw nodes, resolve pass wires cross-references | zeep pattern: pass 1 via parse_* functions returning raw structs, pass 2 via resolver resolving QName strings to concrete references |
| WSDL-03 | Import resolution — recursively loads wsdl:import targets, caches by namespace/location, handles diamond imports and cycles | zeep: import guard "if self._resolved_imports: return"; SchemaLoader trait abstracts file I/O |
| WSDL-04 | WSDL serving on GET ?wsdl returns WSDL XML with soap:address location rewritten to match server's actual URL | base_url config field + Host header fallback; rewrite only the soap:address location= attribute value |
| WSDL-05 | Imported XSD schemas are either inlined or served at their own URLs via GET | Per import tracking at startup; option A: inline all schemas; option B: serve each at schema URL |
| ENV-01 | Parse SOAP 1.2 envelope — extract Header children and Body first child element | quick-xml NsReader streaming; stop at Body first child for QName extraction; collect all ancestor namespace bindings via reader.resolver().bindings() |
| ENV-02 | Serialize SOAP 1.2 response envelope wrapping handler output | quick-xml Writer; write Envelope+Body wrapper around handler's raw bytes |
| ENV-03 | Detect SOAP version from request Content-Type (application/soap+xml = 1.2, text/xml = 1.1) | Parse Content-Type header before XML parsing; SoapVersion enum controls all downstream namespace strings |
| ENV-04 | Set correct response Content-Type header matching the request's SOAP version | Mirror incoming version; application/soap+xml for 1.2, text/xml for 1.1 |
| FLT-01 | Generate spec-correct SOAP 1.2 faults with Code/Value, Reason/Text, and optional Detail | SoapFault struct + FaultCode enum in DESIGN.md; SOAP 1.2 ns: http://www.w3.org/2003/05/soap-envelope |
| FLT-02 | Support standard fault codes: VersionMismatch, MustUnderstand, DataEncodingUnknown, Sender, Receiver | FaultCode enum covers all; MustUnderstand fault triggered when unknown mustUnderstand="1" header not processed |
| FLT-03 | Return HTTP 500 for SOAP 1.2 faults (per W3C SOAP 1.2 spec Section 7.4.2) | axum StatusCode::INTERNAL_SERVER_ERROR; no exceptions |
| DSP-01 | Document/literal dispatch — route requests by body element QName to registered handler | HashMap<QName, Arc<dyn SoapHandler>>; O(1) lookup on (namespace, local_name) |
| DSP-02 | SOAPAction header used as secondary dispatch hint when body element alone is ambiguous | Second HashMap<String, Arc<dyn SoapHandler>> keyed on SOAPAction value; fallback only |
| DSP-03 | Dispatch table built at startup from parsed WSDL — no per-request WSDL interpretation | DispatchTable built in dispatch::build_table(&model, &handlers) once; stored in axum State |
| DSP-04 | Unmatched operations produce a SOAP Fault (action not supported) | FaultCode::Sender with reason "Action not supported" when QName lookup returns None |
| HDL-01 | Raw handler trait — receives XML bytes of request body element, returns XML bytes of response body element or SoapFault | Handler receives (body_bytes: Bytes, ns_map: NamespaceMap) tuple — see handler boundary research |
| HDL-02 | Async handler support (handler returns a Future) | async_trait or RPITIT (Rust 1.75+) on SoapHandler trait |
| HDL-03 | Closure-based handler registration for ergonomic API | Server::handler("OpName", |bytes, ns_map| async move { ... }) via FnHandler wrapper struct implementing SoapHandler |
| SEC-01 | Extract wsse:Security header from SOAP Header | quick-xml streaming: match {http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd}Security |
| SEC-02 | WS-Security UsernameToken PasswordDigest validation | digest = base64(sha1(b64decode(nonce) ++ created_utf8 ++ password_utf8)); test with known vector before implementation |
| SEC-03 | WS-Security UsernameToken PasswordText validation | Direct string comparison; document that PasswordText requires TLS |
| SEC-04 | Timestamp validation with configurable tolerance (default 300 seconds) | chrono: parse wsu:Created as ISO 8601; reject if abs(now - created) > tolerance |
| SEC-05 | Nonce replay prevention with time-windowed cache (rotating bucket design, default 300s window) | Two HashSet<String> buckets, rotate every 150s; nonce in either bucket = replay |
| SEC-06 | Per-operation auth bypass — configurable whitelist of operations that skip WS-Security | HashSet<String> of operation names; check before WS-Security parse (not after auth failure) |
| SEC-07 | Reject unauthenticated requests with SOAP Fault on auth failure | FaultCode::Sender with MustUnderstand variant for unprocessed wsse:Security header |
| HTTP-01 | axum Router integration — server returns axum::Router composable with other routes | SoapService::into_router() -> axum::Router; consumer calls app.merge(soap.into_router()) |
| HTTP-02 | POST handler for SOAP requests on configured path | axum::routing::post(soap_handler).layer(DefaultBodyLimit::max(N)) |
| HTTP-03 | GET handler for WSDL serving on same path with ?wsdl query parameter | axum::routing::get(wsdl_handler) on same path; extract Query<HashMap<String,String>> and check for "wsdl" key |
| HTTP-04 | Server builder API — Server::from_wsdl(wsdl).handler(...).auth(...).build()? | Builder pattern; build() validates all registered operations exist in the WSDL model |
</phase_requirements>

## Summary

Phase 1 is a complete SOAP 1.2 server stack, greenfield in Rust, porting algorithms from three reference implementations. The architecture has been fully pre-researched in `.planning/research/` — the build order, data structures, and anti-patterns are all established. This phase research focuses on the four Claude's Discretion items and the specific source-level details of the reference implementations needed for correct porting.

**Primary recommendation:** Build in strict dependency order (xsd/types.rs → xsd/parser.rs → xsd/resolver.rs → wsdl/ → model.rs → dispatch.rs → envelope.rs → fault.rs → wssec/ → router.rs) with test fixtures written before each layer is implemented. The four discretion decisions have clear answers from the research below.

The handler namespace boundary problem has a concrete solution: use `reader.resolver().bindings()` from quick-xml 0.39 NsReader to collect all in-scope namespace declarations at the Body child element, then prepend them as xmlns attributes when writing the extracted fragment bytes. This is simpler than a (bytes, namespace_map) tuple API and preserves XML self-containedness.

The auth model should support a credential lookup function (`Fn(&str) -> Option<String>`) rather than a static credential pair, because ONVIF devices maintain multiple users (GetUsers/SetUser operations). The ONVIF Core Spec explicitly defines multi-user device authentication. A static credential is a special case of a lookup function and can be expressed as one.

WSDL loading API should support from_file, from_bytes, and from_str — matching zeep's transport-abstracted loader pattern. Embedded WSDLs are a valid use case for test fixtures and deployment scenarios where the WSDL is compiled into the binary.

Test fixtures: bundle the ONVIF devicemgmt.wsdl + onvif.xsd + common.xsd in the repo under tests/fixtures/. These three files together exercise all required XSD features (extension, restriction, xs:any, xs:list, multi-file import). Downloading at test time adds CI fragility.

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| roxmltree | 0.21.1 | WSDL/XSD DOM parsing at startup | Read-only tree API; fastest Rust DOM library; zero-copy; ideal for two-pass parse+resolve |
| quick-xml | 0.39.2 | Per-request SOAP envelope streaming parse/write | SAX-style streaming; no per-request heap allocation; NsReader handles namespace resolution; supports both read and write |
| axum | 0.8.8 | HTTP server framework and Router integration | Dominant Rust web framework; composes via Router::merge; built on tower for middleware |
| tokio | 1.50.0 | Async runtime | Required by axum; tokio::sync::Mutex for nonce cache |
| sha1 (RustCrypto) | 0.11.0 | WS-Security PasswordDigest SHA-1 computation | Pure Rust; no C build step; OASIS spec mandates SHA-1 for PasswordDigest |
| base64 | 0.22.1 | Encode/decode nonce and digest in WS-Security headers | Standard encoding; nonce and digest are Base64 in SOAP XML |
| chrono | 0.4.44 | Parse and validate wsu:Created timestamps | ISO 8601 parsing; freshness window comparison |
| thiserror | 2.0.18 | Derive Error for SoapFault and internal error types | Idiomatic library crate error handling |
| bytes | 1.11.1 | Zero-copy body passing through axum handler | axum body extraction returns Bytes; avoids copies |
| http-body-util | 0.1.3 | BodyExt::collect for buffering request body | Required pattern for reading raw bytes in axum 0.7+ |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| uuid | 1.23.0 | Generate unique nonces if server generates nonces | Only if server needs to generate nonces; client normally supplies them |
| tower | 0.5.3 | Service trait wrapping | Only if exposing tower Service in public API |

### Installation

```toml
[dependencies]
roxmltree        = "0.21"
quick-xml        = { version = "0.39", features = [] }
axum             = "0.8"
tokio            = { version = "1", features = ["sync"] }
sha1             = "0.11"
base64           = "0.22"
chrono           = { version = "0.4", features = ["std"], default-features = false }
thiserror        = "2"
bytes            = "1"
http-body-util   = "0.1"

[dev-dependencies]
tokio            = { version = "1", features = ["full", "test-util"] }
axum-test        = "0.16"
```

## Architecture Patterns

### Recommended Project Structure

```
src/
├── lib.rs                  # Public API: re-exports SoapService, SoapHandler, SoapFault, Auth
├── server.rs               # Server builder; ServerBuilder<Configured/Unconfigured> state machine
├── dispatch.rs             # DispatchTable: QName -> Arc<dyn SoapHandler>; build_table(); route()
├── envelope.rs             # SOAP envelope parse + serialize (quick-xml NsReader/Writer)
├── fault.rs                # SoapFault struct; FaultCode enum; serialize_fault(fault, version)
├── security.rs             # WS-Security orchestration: validate_token(header, config, nonce_cache)
├── wssec/
│   ├── mod.rs              # validate_token() public entry; UsernameTokenConfig struct
│   ├── username_token.rs   # parse_username_token(); verify_digest(); verify_text()
│   ├── nonce_cache.rs      # RotatingNonceCache; two-bucket design; check_and_insert()
│   └── timestamp.rs        # parse_created(); check_freshness(created, tolerance_secs)
├── wsdl/
│   ├── mod.rs              # load_wsdl(source: WsdlSource) -> Result<WsdlModel>; WsdlSource enum
│   ├── parser.rs           # Pass 1: roxmltree DOM -> RawDefinition; pure functions per element type
│   ├── resolver.rs         # Pass 2: forward-ref resolution; import walking; schema delegation
│   └── definitions.rs      # WsdlDefinition, Message, PortType, Binding, Operation, Service, Port
└── xsd/
    ├── mod.rs              # parse_schema(nodes, registry) -> Result<TypeRegistry>
    ├── parser.rs           # Pass 1: roxmltree Node -> RawTypeGraph; visit_* functions
    ├── resolver.rs         # Pass 2: resolve $ref, extension chains, restriction chains, imports
    ├── types.rs            # XsdType, ComplexType, SimpleType, Restriction, ListDef, UnionDef
    └── elements.rs         # XsdElement, XsdAttribute, AttributeGroup, Group, MaxOccurs
```

### Pattern 1: Two-Pass Parse (Parse + Resolve)

**What:** Pass 1 reads XML into intermediate structs with unresolved QName strings. Pass 2 walks the graph and resolves all references.

**When to use:** All of WSDL and XSD — both have forward references and cross-file imports.

**python-zeep reference:** `src/zeep/wsdl/parse.py` (5 pure functions for pass 1) + `src/zeep/wsdl/definitions.py` (data classes) + `src/zeep/wsdl/wsdl.py` (import resolution orchestration).

```rust
// Pass 1 output — xsd/types.rs
struct RawComplexType {
    name: Option<String>,
    extension_base: Option<String>,    // "tns:DeviceEntity" — not yet resolved
    restriction_base: Option<String>,
    content: RawComplexContent,
}

// Pass 2 output — xsd/types.rs (resolved)
struct ComplexType {
    name: Option<String>,
    // extension chain is flattened: all ancestor elements prepended
    all_elements: Vec<XsdElement>,
    attributes: Vec<XsdAttribute>,
}
```

### Pattern 2: XSD Extension Chain Flattening (zeep-faithful port)

**What:** When resolving a ComplexType with extension/restriction, recursively resolve the base type first, then merge. Memoize with a `resolved: bool` flag.

**Critical detail from zeep `xsd/types/complex.py`:** The `resolve()` method calls `self._extension.resolve()` on the base type before calling `extend()`. This creates bottom-up flattening — every ancestor type in the chain is fully resolved before being merged into the child. The `extend()` method prepends the base type's elements to the child's own elements.

```rust
// xsd/resolver.rs
fn resolve_complex_type(
    raw: &RawComplexType,
    registry: &mut TypeRegistry,
    resolving: &mut HashSet<QName>,  // cycle detection
) -> Result<ComplexType> {
    if let Some(base_qname) = &raw.extension_base {
        // Resolve base first (recursive) before merging
        let base = resolve_named_type(base_qname, registry, resolving)?;
        let mut elements = base.all_elements.clone();  // prepend ancestor elements
        elements.extend(resolve_content(&raw.content, registry, resolving)?);
        Ok(ComplexType { all_elements: elements, attributes: /* merged */ })
    } else {
        // No extension — resolve content directly
        Ok(ComplexType {
            all_elements: resolve_content(&raw.content, registry, resolving)?,
            attributes: resolve_attributes(&raw.attributes, registry, resolving)?,
        })
    }
}
```

**Test fixture required before implementing:** 3-level inheritance chain: BaseType → MiddleType extends BaseType → LeafType extends MiddleType. After resolution, LeafType must contain elements from all three levels.

### Pattern 3: Dispatch by Body First-Child QName

**What:** Extract QName of SOAP Body's first child element, look up in a HashMap built at startup.

**node-soap reference:** `src/server.ts` uses `topElements` map from body element local name to handler. Note: node-soap works with deserialized objects (loses namespace context). Our implementation works with raw bytes — see handler boundary section.

```rust
// dispatch.rs
pub struct DispatchTable {
    by_element: HashMap<QName, Arc<dyn SoapHandler>>,
    by_action: HashMap<String, Arc<dyn SoapHandler>>,  // secondary fallback
}

pub fn route<'a>(
    table: &'a DispatchTable,
    body_first_child_qname: &QName,
    soap_action: Option<&str>,
) -> Result<&'a Arc<dyn SoapHandler>, SoapFault> {
    if let Some(h) = table.by_element.get(body_first_child_qname) {
        return Ok(h);
    }
    if let Some(action) = soap_action {
        if let Some(h) = table.by_action.get(action) {
            return Ok(h);
        }
    }
    Err(SoapFault::action_not_supported(body_first_child_qname))
}
```

### Pattern 4: Namespace Context Preservation at Handler Boundary (RESOLVED)

**What:** When extracting the Body's first child element as bytes to pass to the raw handler, ancestor namespace declarations from the Envelope element are lost. The handler's parser sees unbound prefixes.

**Research finding:** quick-xml 0.39 NsReader exposes `reader.resolver().bindings()` which returns a `NamespaceBindingsIter` yielding `(PrefixDeclaration, Namespace)` tuples — all in-scope namespace bindings at the current parse position. This gives us everything needed to re-emit namespace declarations on the extracted fragment root.

**Recommended approach:** Re-emit all in-scope namespace declarations as explicit `xmlns:prefix="uri"` attributes on the extracted body element's start tag. This keeps the handler API simple — just `&[u8]` in — and the extracted bytes are self-contained valid XML.

```rust
// envelope.rs — extracting body child with namespace context
fn extract_body_child_with_ns(reader: &mut NsReader<&[u8]>) -> Result<Bytes> {
    // At Body child start tag: collect all in-scope bindings
    let bindings: Vec<(String, String)> = reader.resolver()
        .bindings()
        .map(|(prefix, ns)| (prefix.to_string(), ns.as_str().to_string()))
        .collect();

    // Write the start tag with all namespace declarations injected
    let mut out = Vec::new();
    let mut writer = Writer::new(&mut out);
    // ... write start event with bindings injected as xmlns attributes
    // ... copy remaining events until matching End
    Ok(Bytes::from(out))
}
```

**Handler trait using resolved approach:**
```rust
// handler.rs
#[async_trait]
pub trait SoapHandler: Send + Sync + 'static {
    /// Receives XML bytes of the body element with all ancestor namespace
    /// declarations re-emitted on the root. Returns XML bytes of the response
    /// body element, or a SoapFault.
    async fn handle(&self, body: Bytes) -> Result<Bytes, SoapFault>;
}
```

### Pattern 5: Security as Pre-Dispatch Interceptor

**What:** WS-Security validation runs before dispatch. Bypass list checked first — before attempting WS-Security parse.

**ONVIF context:** Per ONVIF Core Spec and community documentation, `GetSystemDateAndTime`, `GetServices`, `GetServiceCapabilities`, `GetCapabilities`, and `GetHostname` are required to be accessible without authentication. `GetSystemDateAndTime` is the critical one because clients need it to compute PasswordDigest (clock skew adjustment).

```rust
// server.rs (per-request pipeline)
async fn handle_soap_request(/* axum state + body */) -> Response {
    let envelope = envelope::parse(&body)?;

    // Check bypass BEFORE attempting WS-Security
    let operation_qname = dispatch::extract_body_first_child_qname(&envelope.body)?;
    let requires_auth = !state.auth_bypass.contains(operation_qname.local_name());

    if requires_auth {
        wssec::validate_token(
            &envelope.header,
            &state.auth_config,
            &state.nonce_cache,
        )?;
    }

    let handler = dispatch::route(&state.dispatch_table, &operation_qname, soap_action)?;
    let response_body = handler.handle(envelope.body_bytes).await?;
    envelope::serialize_response(response_body, soap_version)
}
```

### Pattern 6: WS-Security PasswordDigest (rpos-faithful port)

**Formula (OASIS WS-UsernameToken Profile 1.1.1):**
```
PasswordDigest = Base64(SHA-1(Base64Decode(Nonce) || Created_UTF8 || Password_UTF8))
```

**Critical:** The Nonce element value in the XML is Base64-encoded. It MUST be decoded to raw bytes before concatenation. Concatenating the Base64 string instead of its decoded bytes produces a digest that never matches.

```rust
// wssec/username_token.rs
fn verify_digest(
    nonce_b64: &str,
    created: &str,
    password_utf8: &str,
    provided_digest: &str,
) -> bool {
    use sha1::{Sha1, Digest};

    let nonce_bytes = base64::decode(nonce_b64).expect("invalid nonce base64");
    let mut hasher = Sha1::new();
    hasher.update(&nonce_bytes);           // decoded bytes
    hasher.update(created.as_bytes());     // UTF-8 bytes
    hasher.update(password_utf8.as_bytes()); // UTF-8 bytes
    let digest = hasher.finalize();
    let expected = base64::encode(&digest);
    expected == provided_digest
}
```

**Test vector required:** Must write a unit test with a hard-coded `(nonce_b64, created, password, expected_digest)` from the rpos reference implementation BEFORE writing any PasswordDigest code.

### Pattern 7: Rotating Nonce Cache

**What:** Two HashSet<String> buckets. A nonce in either bucket is a replay. Rotate every T/2 seconds (T = freshness window, default 300s → rotate every 150s).

```rust
// wssec/nonce_cache.rs
pub struct RotatingNonceCache {
    current: HashSet<String>,
    previous: HashSet<String>,
    last_rotation: Instant,
    rotation_interval: Duration,  // freshness_window / 2
}

impl RotatingNonceCache {
    pub fn check_and_insert(&mut self, nonce: &str) -> bool {
        self.maybe_rotate();
        if self.current.contains(nonce) || self.previous.contains(nonce) {
            return false;  // replay
        }
        self.current.insert(nonce.to_string());
        true  // accepted
    }

    fn maybe_rotate(&mut self) {
        if self.last_rotation.elapsed() >= self.rotation_interval {
            self.previous = std::mem::take(&mut self.current);
            self.last_rotation = Instant::now();
        }
    }
}
```

### Pattern 8: Auth Credential Lookup Function

**Recommended decision:** Use a credential lookup function (`Box<dyn Fn(&str) -> Option<String> + Send + Sync>`) rather than a static (username, password) pair.

**Rationale from research:**
- ONVIF Core Spec defines `GetUsers`/`SetUser`/`CreateUsers`/`DeleteUsers` operations — ONVIF devices maintain multiple users
- The onvif-server consumer will need to proxy these calls and maintain its own user store
- A lookup function covers both single-credential (constant closure) and multi-user (HashMap lookup) cases
- Using a static pair creates a breaking API change when the first consumer needs multi-user

```rust
// server.rs
pub struct Auth {
    /// Returns the password for the given username, or None if user not found
    pub credential_fn: Box<dyn Fn(&str) -> Option<String> + Send + Sync>,
    pub bypass_operations: HashSet<String>,
    pub timestamp_tolerance_secs: u64,
    pub nonce_window_secs: u64,
}

impl Auth {
    /// Convenience constructor for single-credential case
    pub fn single(username: impl Into<String>, password: impl Into<String>) -> Self {
        let u = username.into();
        let p = password.into();
        Auth {
            credential_fn: Box::new(move |name| {
                if name == u { Some(p.clone()) } else { None }
            }),
            ..Default::default()
        }
    }
}
```

### Pattern 9: WSDL Loading API

**Recommended decision:** Support three loading paths via a `WsdlSource` enum.

**Rationale:** Tests need from_str/from_bytes (embedded fixture strings). Production deployments may compile the WSDL into the binary. from_file is the common case. python-zeep uses a transport abstraction to support all three.

```rust
// wsdl/mod.rs
pub enum WsdlSource<'a> {
    File(PathBuf),
    Bytes(Cow<'a, [u8]>),
    Str(Cow<'a, str>),
}

pub fn load_wsdl(source: WsdlSource) -> Result<WsdlModel> {
    let bytes = match source {
        WsdlSource::File(path) => std::fs::read(path)?,
        WsdlSource::Bytes(b) => b.into_owned(),
        WsdlSource::Str(s) => s.as_bytes().to_vec(),
    };
    // ... schema loader for resolving imports from filesystem relative to file path
}
```

### Pattern 10: Test Fixtures Decision

**Recommended decision:** Bundle ONVIF WSDLs in the repo under `tests/fixtures/onvif/`.

**Rationale from research:** The ONVIF WSDLs needed are:
- `devicemgmt.wsdl` — primary device management WSDL; 60+ operations; xs:import to onvif.xsd
- `onvif.xsd` — all XSD features: extension (3 levels), restriction enumerations, xs:any, xs:anyAttribute, xs:list, xs:import to common.xsd
- `common.xsd` — base types (PTZ, geometric shapes, ReferenceToken); xs:restriction from xs:string

These three files together exercise every required XSD feature in requirements XSD-01 through XSD-10. Downloading at test time creates CI fragility (ONVIF servers have been unreliable). The files are small (<200KB total) and their licenses permit redistribution.

**Additional fixture needed for XSD-11 validation:** One synthetic XSD file with a 3-level inheritance chain (BaseType → MiddleType → LeafType) to verify XSD-03 and XSD-11 before using real ONVIF WSDLs.

```
tests/
├── fixtures/
│   ├── onvif/
│   │   ├── devicemgmt.wsdl
│   │   ├── onvif.xsd
│   │   └── common.xsd
│   ├── soap/
│   │   ├── valid_request_1_2.xml         # Valid SOAP 1.2 envelope
│   │   ├── fault_request_1_2.xml         # Request that triggers fault
│   │   └── wssec_request_digest.xml      # Valid WS-Security PasswordDigest request
│   └── xsd/
│       ├── three_level_inheritance.xsd   # BaseType -> MiddleType -> LeafType
│       └── all_features.xsd             # xs:any, xs:list, xs:union, xs:group, xs:attributeGroup
```

### Anti-Patterns to Avoid

- **Single-pass WSDL/XSD parsing:** Forward references in ONVIF WSDLs require two-pass. This is not a shortcut; it is a correctness failure.
- **Dispatching on SOAPAction header alone:** ONVIF uses document/literal; many clients send empty SOAPAction. Body element QName is the mandatory primary key.
- **Putting WS-Security inside handlers:** Auth bypass for GetSystemDateAndTime becomes scattered. Pre-dispatch interceptor only.
- **Using roxmltree for per-request parsing:** roxmltree is DOM-based and allocates; use quick-xml streaming on the hot path.
- **Checking auth bypass after auth failure:** Timing side-channel. Check bypass list first, before any WS-Security parsing.
- **Bare `HashSet<String>` for nonce cache:** Memory leak under sustained load. Two-bucket rotating design from the start.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| XML DOM traversal at startup | Custom XML parser for WSDL | roxmltree 0.21 | Correctly handles all XML features; battle-tested on WSDL/XSD; namespace resolution built in |
| Per-request XML streaming | Custom SAX parser | quick-xml NsReader | Correctly handles namespace prefix binding/unbinding as element scope changes; exposes resolver().bindings() |
| SHA-1 hashing | Custom SHA-1 | sha1 (RustCrypto) | SHA-1 is not trivial to implement correctly; OASIS spec defines exact byte-level behavior |
| Base64 encode/decode | Custom base64 | base64 0.22 | Subtle variant differences (standard vs URL-safe); nonce in SOAP uses standard variant |
| Timestamp parsing | Custom ISO 8601 parser | chrono 0.4 | ISO 8601 has many valid forms; chrono handles them correctly |
| HTTP routing | Custom HTTP dispatch | axum Router | axum Router composition is the entire point; no benefit to custom routing |
| Async trait objects | Manual boxing/pinning | async_trait or RPITIT (1.75+) | async fn in traits requires care; established patterns exist |

**Key insight:** The complexity budget for this phase is in the WSDL/XSD parsing logic and WS-Security implementation. Everything else should be delegated to established crates to keep the scope manageable.

## Common Pitfalls

### Pitfall 1: Namespace Inheritance Loss When Extracting Body Bytes
**What goes wrong:** Body element bytes passed to handler have no ancestor namespace declarations from `<Envelope>`. Handler's parser sees "prefix not bound" errors on real ONVIF messages.
**Why it happens:** ONVIF messages declare most namespaces on the Envelope root, not on individual elements.
**How to avoid:** Use `reader.resolver().bindings()` from quick-xml 0.39 NsReader to collect in-scope bindings at the Body child start tag. Inject as `xmlns:prefix="uri"` attributes when re-serializing the fragment. Decision: re-emit on fragment root (not tuple API) for simpler handler interface.
**Warning signs:** Tests pass with minimal SOAP envelopes but fail with real ONVIF messages.

### Pitfall 2: XSD Extension Chain Incomplete
**What goes wrong:** Multi-level inheritance (ONVIF: VideoSource extends DeviceEntity) missing base type fields.
**Why it happens:** Resolver handles one level but doesn't recursively flatten ancestors.
**How to avoid:** port zeep's `resolve()` pattern: call `base_type.resolve()` recursively before merging. Write 3-level test fixture before implementing. ONVIF has at least 2-level chains (ConfigurationEntity → VideoEncoder2Configuration).
**Warning signs:** Simple types pass tests; types using extension fail.

### Pitfall 3: PasswordDigest Nonce Byte Order Error
**What goes wrong:** All PasswordDigest authentications fail; PasswordText succeeds.
**Why it happens:** Nonce XML value is Base64-encoded; must be decoded to raw bytes before concatenation with created and password.
**How to avoid:** Write unit test with known-good (nonce, created, password, digest) vector from rpos BEFORE writing verify_digest(). Formula: `Base64(SHA-1(B64DECODE(nonce) ++ created_bytes ++ password_bytes))`.
**Warning signs:** No unit test with hard-coded expected digest value.

### Pitfall 4: WSDL Import Resolution Silent Failure
**What goes wrong:** Startup succeeds but operations fail at runtime with "type unknown".
**Why it happens:** XSD import fails (wrong relative path) and is treated as a warning.
**How to avoid:** Treat unresolved xs:import or wsdl:import as a hard startup error. Implement SchemaLoader trait for flexible file resolution. Test with ONVIF's multi-file set (devicemgmt.wsdl + onvif.xsd + common.xsd).
**Warning signs:** Tests only use single-file WSDLs.

### Pitfall 5: SOAPAction as Primary Dispatch Key
**What goes wrong:** Operations work in SoapUI (sends SOAPAction) but fail with real ONVIF cameras (may omit SOAPAction or send empty string).
**Why it happens:** Document/literal spec says body element QName is the dispatch key; SOAPAction is advisory.
**How to avoid:** Primary dispatch on body element QName. SOAPAction as secondary fallback only.
**Warning signs:** Empty SOAPAction causes "action not supported" fault.

### Pitfall 6: Nonce Cache Unbounded Growth
**What goes wrong:** Memory grows continuously on ONVIF-polled devices (frequent requests).
**Why it happens:** `HashSet<String>` with no expiry.
**How to avoid:** Rotating two-bucket design from the start. No API change needed later.
**Warning signs:** Memory grows after 1000+ authentication requests.

### Pitfall 7: roxmltree DOCTYPE Rejection
**What goes wrong:** "Unexpected token" or panic at startup with non-ONVIF WSDLs containing DOCTYPE declarations.
**Why it happens:** roxmltree known limitation (GitHub issue #56).
**How to avoid:** Strip `<!DOCTYPE ...>` declarations before passing bytes to roxmltree. Emit a structured warning.
**Warning signs:** Non-ONVIF WSDLs fail to parse; no clear error message.

### Pitfall 8: WS-Security MustUnderstand Not Enforced
**What goes wrong:** Unknown headers with `mustUnderstand="1"` pass through silently.
**Why it happens:** Happy-path header processing only; unprocessed mustUnderstand headers not checked.
**How to avoid:** After processing all known headers, any remaining `mustUnderstand="1"` header targeted at the current role must produce a `MustUnderstand` fault.
**Warning signs:** WS-Security errors produce generic `Sender` faults instead of `MustUnderstand`.

### Pitfall 9: axum Body Size Limit Silent Truncation
**What goes wrong:** Large SOAP messages (or ONVIF video configuration payloads) are silently truncated at 2MB default.
**How to avoid:** Set `DefaultBodyLimit::max(N)` or `DefaultBodyLimit::disable()` on the SOAP route. Document the default in the crate.

## Code Examples

### WSDL Parsing — Two-Pass Structure (from zeep parse.py port)

```rust
// wsdl/parser.rs — Pass 1: pure functions, XML node -> raw struct
// Source: python-zeep src/zeep/wsdl/parse.py

fn parse_message(node: roxmltree::Node) -> Result<RawMessage> {
    Ok(RawMessage {
        name: node.attribute("name").ok_or(Error::MissingAttr("name"))?.to_string(),
        parts: node.children()
            .filter(|n| n.tag_name().name() == "part")
            .map(parse_message_part)
            .collect::<Result<Vec<_>>>()?,
    })
}

fn parse_message_part(node: roxmltree::Node) -> Result<RawMessagePart> {
    Ok(RawMessagePart {
        name: node.attribute("name").ok_or(Error::MissingAttr("name"))?.to_string(),
        element: node.attribute("element").map(str::to_string),  // document style: QName string
        type_ref: node.attribute("type").map(str::to_string),    // RPC style: QName string
    })
}
```

### XSD Visitor — visit_* Pattern (from zeep xsd/visitor.py port)

```rust
// xsd/parser.rs — Pass 1: visit functions per XSD element type
// Source: python-zeep src/zeep/xsd/visitor.py

fn visit_complex_type(
    node: roxmltree::Node,
    target_ns: Option<&str>,
) -> Result<RawComplexType> {
    let name = node.attribute("name").map(str::to_string);
    let content = if let Some(cc) = node.children().find(|n| n.tag_name().name() == "complexContent") {
        visit_complex_content(cc, target_ns)?
    } else if let Some(sc) = node.children().find(|n| n.tag_name().name() == "simpleContent") {
        visit_simple_content(sc, target_ns)?
    } else {
        // Direct sequence/choice/all children
        visit_content_group(node, target_ns)?
    };
    Ok(RawComplexType { name, content })
}

fn visit_extension_complex_content(
    node: roxmltree::Node,
    target_ns: Option<&str>,
) -> Result<RawExtension> {
    // Source: zeep visit_extension_complex_content — extract base QName string
    let base = resolve_qname(
        node.attribute("base").ok_or(Error::MissingAttr("base"))?,
        node,
        target_ns,
    );
    let child_content = visit_content_group(node, target_ns)?;
    let attributes = collect_attributes(node, target_ns)?;
    Ok(RawExtension { base, content: child_content, attributes })
}
```

### Envelope Parsing — quick-xml NsReader

```rust
// envelope.rs — streaming parse extracting Header and Body bytes
// Source: quick-xml 0.39 NsReader API

pub struct ParsedEnvelope {
    pub soap_version: SoapVersion,
    pub header_children: Vec<HeaderChild>,  // each with namespace context
    pub body_bytes: Bytes,                  // self-contained XML fragment
    pub body_qname: QName,                  // first child QName for dispatch
}

pub fn parse_envelope(body_bytes: &[u8], content_type: &str) -> Result<ParsedEnvelope> {
    let version = detect_soap_version(content_type)?;
    let envelope_ns = version.envelope_namespace();

    let mut reader = NsReader::from_reader(body_bytes);
    let mut state = EnvelopeParseState::BeforeEnvelope;

    loop {
        match reader.read_resolved_event()? {
            (ResolveResult::Bound(ns), Event::Start(e)) if ns.as_ref() == envelope_ns.as_bytes() => {
                match e.local_name().as_ref() {
                    b"Envelope" => state = EnvelopeParseState::InEnvelope,
                    b"Header" if state == InEnvelope => state = InHeader,
                    b"Body" if state == InEnvelope | AfterHeader => {
                        // Collect all in-scope bindings to inject into body child
                        let bindings = collect_ns_bindings(&reader);
                        return parse_body_child(&mut reader, bindings, version);
                    }
                    _ => {}
                }
            }
            (_, Event::Eof) => return Err(Error::UnexpectedEof),
            _ => {}
        }
    }
}

fn collect_ns_bindings(reader: &NsReader<&[u8]>) -> Vec<(String, String)> {
    // quick-xml 0.39: reader.resolver().bindings()
    reader.resolver()
        .bindings()
        .map(|(prefix, ns)| (prefix.to_string(), ns.as_str().to_string()))
        .collect()
}
```

### WS-Security Extraction from SOAP Header

```rust
// wssec/username_token.rs
// WS-Security namespace constants
const WSSE_NS: &str = "http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd";
const WSU_NS: &str = "http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-utility-1.0.xsd";
const PASSWORD_DIGEST_TYPE: &str = "http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-username-token-profile-1.0#PasswordDigest";
const PASSWORD_TEXT_TYPE: &str = "http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-username-token-profile-1.0#PasswordText";
const NONCE_B64_TYPE: &str = "http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-soap-message-security-1.0#Base64Binary";

pub struct UsernameToken {
    pub username: String,
    pub password: String,
    pub password_type: PasswordType,  // Digest or Text
    pub nonce_b64: String,
    pub created: String,
}
```

### SOAP Fault Generation — Version-Aware

```rust
// fault.rs
// SOAP 1.2 fault (FLT-01, FLT-02, FLT-03)
pub fn serialize_fault_12(fault: &SoapFault) -> Bytes {
    // Source: W3C SOAP 1.2 Part 1, Section 5.4
    let ns = "http://www.w3.org/2003/05/soap-envelope";
    format!(r#"<?xml version="1.0"?>
<env:Envelope xmlns:env="{ns}">
  <env:Body>
    <env:Fault>
      <env:Code><env:Value>env:{code}</env:Value></env:Code>
      <env:Reason><env:Text xml:lang="en">{reason}</env:Text></env:Reason>
      {detail}
    </env:Fault>
  </env:Body>
</env:Envelope>"#,
        ns = ns,
        code = fault.code.to_soap12_value(),
        reason = escape_xml(&fault.reason),
        detail = fault.detail.as_ref().map(|d| format!("<env:Detail>{}</env:Detail>", d)).unwrap_or_default(),
    ).into()
}
```

## State of the Art

| Old Approach | Current Approach | Impact |
|--------------|------------------|--------|
| `.prefixes()` on NsReader | `.resolver().bindings()` | quick-xml 0.39 removed deprecated `.prefixes()`; must use new API |
| async_trait crate for async trait methods | RPITIT (return-position impl Trait, Rust 1.75+) | Can use either; async_trait still works and is more compatible |
| Static credentials in auth config | Credential lookup function | Required for multi-user ONVIF device support |
| from_file() only WSDL loading | WsdlSource enum (file/bytes/str) | Enables embedded WSDLs in tests and compiled binaries |

**Deprecated/outdated:**
- `NsReader::prefixes()`: Removed in quick-xml 0.39 — use `resolver().bindings()`
- `NsReader::resolve()`: Removed in quick-xml 0.39 — use `resolver().resolve()`
- Static single-credential auth: Insufficient for ONVIF multi-user spec compliance

## Open Questions

1. **ONVIF WSDL schema loading: relative vs absolute paths**
   - What we know: devicemgmt.wsdl imports `../../../ver10/schema/onvif.xsd` with a relative path
   - What's unclear: The SchemaLoader trait needs to resolve relative paths from the importing WSDL's location. How to track "current file base path" through recursive imports.
   - Recommendation: Pass `base_path: Option<PathBuf>` through the import resolution chain. When loading from bytes/str (no base path), relative imports fail with a clear error directing users to provide file paths.

2. **WSDL-05: How to serve imported XSD schemas**
   - What we know: Requirement says "inlined or served at their own URLs"
   - What's unclear: The simpler option (inline all schemas into the WSDL response) avoids serving multiple URLs but may not work for clients that fetch schemas separately.
   - Recommendation: Default to serving imported schemas at their canonical ONVIF URLs (the ones in the `namespace` attribute of xs:import). Document that consumers behind a reverse proxy may need to configure `base_url`.

3. **XSD-11 payload validation: when to enforce**
   - What we know: Validate request body against input schema before handler invocation
   - What's unclear: Should validation failures produce `Sender` fault or a specific schema validation fault? Should validation be opt-in or opt-out?
   - Recommendation: Make validation opt-out via `.skip_validation()` on the builder. Failure produces `FaultCode::Sender` with the validation error in the Detail element.

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust built-in test (`cargo test`) with tokio-test for async |
| Config file | None required — standard Cargo test infrastructure |
| Quick run command | `cargo test --lib` |
| Full suite command | `cargo test` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| XSD-01 | Parse complexType/simpleType/element/attribute from XSD | unit | `cargo test --test xsd_parsing` | Wave 0 |
| XSD-02 | sequence/all/choice content models parse correctly | unit | `cargo test --test xsd_parsing sequence_choice_all` | Wave 0 |
| XSD-03 | 3-level inheritance chain flattens all ancestor elements | unit | `cargo test --test xsd_parsing three_level_extension` | Wave 0 |
| XSD-04 | xs:import with cycle detection loads multi-file schemas | unit | `cargo test --test xsd_parsing import_resolution` | Wave 0 |
| XSD-05 through XSD-10 | Each XSD feature parses and resolves correctly | unit | `cargo test --test xsd_parsing` | Wave 0 |
| XSD-11 | Validation rejects invalid body, accepts valid body | unit | `cargo test --test xsd_validation` | Wave 0 |
| WSDL-01 through WSDL-03 | ONVIF devicemgmt.wsdl loads with all operations resolved | integration | `cargo test --test wsdl_parsing onvif_devicemgmt` | Wave 0 |
| WSDL-04 | GET ?wsdl returns WSDL with rewritten soap:address | integration | `cargo test --test http_integration wsdl_serving` | Wave 0 |
| ENV-01 | SOAP 1.2 envelope parsed, Header and Body extracted | unit | `cargo test --test envelope_parsing soap12_parse` | Wave 0 |
| ENV-02 | Response envelope wraps handler bytes correctly | unit | `cargo test --test envelope_parsing serialize_response` | Wave 0 |
| ENV-03/04 | Content-Type detection routes to correct version | unit | `cargo test --test envelope_parsing version_detection` | Wave 0 |
| FLT-01/02/03 | SOAP 1.2 fault has correct structure; returns HTTP 500 | unit | `cargo test --test fault_generation soap12_fault` | Wave 0 |
| DSP-01 | Body QName dispatch routes to correct handler | unit | `cargo test --test dispatch body_qname_dispatch` | Wave 0 |
| DSP-02 | SOAPAction fallback used when QName unmatched | unit | `cargo test --test dispatch soapaction_fallback` | Wave 0 |
| DSP-03 | Dispatch table built at startup, not per request | unit | `cargo test --test dispatch table_built_once` | Wave 0 |
| DSP-04 | Unmatched operation produces SOAP fault | unit | `cargo test --test dispatch unmatched_operation_fault` | Wave 0 |
| HDL-01 | Handler receives self-contained body bytes | unit | `cargo test --test handler_api raw_handler_bytes` | Wave 0 |
| HDL-02 | Async handler awaited correctly | unit | `cargo test --test handler_api async_handler` | Wave 0 |
| HDL-03 | Closure registration works | unit | `cargo test --test handler_api closure_registration` | Wave 0 |
| SEC-02 | PasswordDigest computed with known test vector | unit | `cargo test --test security password_digest_vector` | Wave 0 |
| SEC-03 | PasswordText direct comparison | unit | `cargo test --test security password_text` | Wave 0 |
| SEC-04 | Expired timestamp rejected; fresh timestamp accepted | unit | `cargo test --test security timestamp_freshness` | Wave 0 |
| SEC-05 | Replayed nonce rejected within window | unit | `cargo test --test security nonce_replay` | Wave 0 |
| SEC-06 | GetSystemDateAndTime bypasses WS-Security | unit | `cargo test --test security auth_bypass` | Wave 0 |
| SEC-07 | Unauthenticated request returns SOAP fault | unit | `cargo test --test security auth_required_fault` | Wave 0 |
| HTTP-01 through HTTP-04 | End-to-end: load WSDL, register handler, send POST, verify response | integration | `cargo test --test integration_e2e` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test --lib`
- **Per wave merge:** `cargo test`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `tests/xsd_parsing.rs` — covers XSD-01 through XSD-10
- [ ] `tests/xsd_validation.rs` — covers XSD-11
- [ ] `tests/wsdl_parsing.rs` — covers WSDL-01 through WSDL-03
- [ ] `tests/http_integration.rs` — covers WSDL-04, WSDL-05, HTTP-01 through HTTP-04
- [ ] `tests/envelope_parsing.rs` — covers ENV-01 through ENV-04
- [ ] `tests/fault_generation.rs` — covers FLT-01 through FLT-03
- [ ] `tests/dispatch.rs` — covers DSP-01 through DSP-04
- [ ] `tests/handler_api.rs` — covers HDL-01 through HDL-03
- [ ] `tests/security.rs` — covers SEC-01 through SEC-07
- [ ] `tests/integration_e2e.rs` — end-to-end pipeline
- [ ] `tests/fixtures/onvif/devicemgmt.wsdl` — ONVIF device management WSDL
- [ ] `tests/fixtures/onvif/onvif.xsd` — ONVIF main schema
- [ ] `tests/fixtures/onvif/common.xsd` — ONVIF common types schema
- [ ] `tests/fixtures/xsd/three_level_inheritance.xsd` — XSD-03 test fixture
- [ ] `tests/fixtures/xsd/all_features.xsd` — comprehensive XSD feature fixture
- [ ] `tests/fixtures/soap/valid_request_1_2.xml` — valid SOAP 1.2 envelope
- [ ] `tests/fixtures/soap/wssec_request_digest.xml` — WS-Security PasswordDigest request

## Sources

### Primary (HIGH confidence)

- python-zeep `src/zeep/wsdl/parse.py` — 5 pure parse functions; QName resolution pattern; import delegation to Definition
- python-zeep `src/zeep/xsd/visitor.py` — complete visit_* method list with signatures; namespace state tracking via _target_namespace; SchemaVisitor class structure
- python-zeep `src/zeep/xsd/types/complex.py` — resolve() recursive chain flattening; memoization pattern; extend() merge logic
- python-zeep `src/zeep/wsdl/wsdl.py` — transport-abstracted loader; import resolution guard; "if self._resolved_imports: return" cycle prevention
- node-soap `src/server.ts` — topElements dispatch map; body extraction via xmlToObject (object-centric, loses namespace — confirms our bytes+ns approach is better)
- [OASIS WS-Security UsernameToken Profile 1.1.1](https://docs.oasis-open.org/wss-m/wss/v1.1.1/os/wss-UsernameTokenProfile-v1.1.1-os.html) — PasswordDigest formula; nonce requirements; 5-minute timestamp window
- [W3C SOAP 1.2 Messaging Framework](https://www.w3.org/TR/soap12-part1/) — mustUnderstand, fault structure, HTTP 500 requirement
- [quick-xml 0.39 NsReader docs.rs](https://docs.rs/quick-xml/latest/quick_xml/reader/struct.NsReader.html) — resolver().bindings() API for namespace context collection
- [quick-xml 0.39 Changelog](https://github.com/tafia/quick-xml/blob/master/Changelog.md) — confirms .prefixes() removed; resolver().bindings() is replacement
- [ONVIF devicemgmt.wsdl](https://www.onvif.org/ver10/device/wsdl/devicemgmt.wsdl) — 60+ operations; imports onvif.xsd; confirms xs:import pattern
- [mictlanix/onvif onvif.xsd](https://github.com/mictlanix/onvif/blob/master/wsdl/onvif.xsd) — extension/restriction confirmed; xs:any, xs:anyAttribute, xs:list confirmed; multi-file import to common.xsd confirmed; max inheritance depth: 3 levels
- [mictlanix/onvif common.xsd](https://github.com/mictlanix/onvif/blob/master/wsdl/common.xsd) — base types (ReferenceToken, Vector, PTZ); xs:restriction from xs:string
- DESIGN.md (docs/DESIGN.md) — authoritative struct definitions; public API; crate structure

### Secondary (MEDIUM confidence)

- [ONVIF Core Spec](https://www.onvif.org/specs/2212/ONVIF-Core-Spec-v2212.pdf) — GetUsers/SetUser multi-user operations; GetSystemDateAndTime auth bypass requirement
- [ONVIF APG Guide](https://www.onvif.org/wp-content/uploads/2016/12/ONVIF_WG-APG-Application_Programmers_Guide-1.pdf) — initial discovery sequence; GetSystemDateAndTime pre-auth call pattern
- [Apache CXF nonce caching](https://coheigea.blogspot.com/2012/04/security-token-caching-in-apache-cxf-26.html) — rotating two-bucket nonce cache design
- [EdgeX ONVIF user auth](https://docs.edgexfoundry.org/3.0/microservices/device/supported/device-onvif-camera/supplementary-info/onvif-user-authentication/) — PasswordDigest formula confirmed; replay protection requirements
- [Dispatch by Body Element — Microsoft WCF](https://learn.microsoft.com/en-us/dotnet/framework/wcf/samples/dispatch-by-body-element) — document/literal dispatch pattern confirmed

### Tertiary (LOW confidence)
- WebSearch: ONVIF auth bypass operations — confirms GetSystemDateAndTime, GetServices, GetCapabilities, GetHostname are pre-auth operations per ONVIF spec (corroborated by ONVIF Core Spec link above → upgraded to MEDIUM)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — all versions locked in CONTEXT.md; versions verified in prior STACK.md research
- Architecture: HIGH — fully pre-researched in ARCHITECTURE.md; four discretion items resolved with source-level evidence
- Handler namespace boundary: HIGH — quick-xml 0.39 NsReader resolver().bindings() API confirmed
- Auth model: HIGH — ONVIF Core Spec multi-user requirement confirmed
- XSD extension chains: HIGH — zeep complex.py resolve() pattern confirmed; ONVIF schemas have 3-level depth confirmed
- Test fixtures: HIGH — ONVIF WSDL file list confirmed by fetching actual WSDL; XSD feature coverage verified
- Pitfalls: HIGH (spec-level) / MEDIUM (Rust-specific) — all critical pitfalls from prior PITFALLS.md confirmed

**Research date:** 2026-04-04
**Valid until:** 2026-07-04 (90 days — stable specs; quick-xml changelog should be rechecked before implementation)
