# soap-server: Rust SOAP Server Crate

## Purpose

A general-purpose, spec-compliant SOAP server crate for Rust, published to crates.io. This fills a gap in the Rust ecosystem — no production SOAP server library exists in Rust.

## License

MIT OR Apache-2.0 (dual licensed, standard Rust ecosystem convention).

## Porting Source

**Primary:** python-zeep (MIT, ~2,000 stars, mvantellingen/python-zeep)

Key source files to port:

| zeep file | Lines | What it provides |
|-----------|-------|-----------------|
| `src/zeep/wsdl/wsdl.py` | 470 | Top-level WSDL document loader, import resolution |
| `src/zeep/wsdl/parse.py` | 232 | 5 pure functions: XML element -> typed struct |
| `src/zeep/wsdl/definitions.py` | 338 | Data classes for WSDL constructs |
| `src/zeep/wsdl/bindings/soap.py` | ~400 | SOAP 1.1/1.2 binding implementations |
| `src/zeep/xsd/schema.py` | ~300 | XSD schema graph |
| `src/zeep/xsd/visitor.py` | ~500 | XSD element/type visitor |
| `src/zeep/xsd/elements/` | ~800 | Element, Attribute, Any, Choice, Sequence |
| `src/zeep/xsd/types/` | ~600 | ComplexType, SimpleType, restriction, extension |

**Supplementary references:**

| Source | What to take |
|--------|-------------|
| node-soap `src/server.ts` (MIT) | Runtime dispatch pattern: `topElements` map from body element name -> handler |
| node-soap `src/wsdl/elements.ts` (MIT) | WSDL element class hierarchy |
| rpos `lib/SoapService.ts` (MIT) | WS-Security UsernameToken digest — cleanest 50-line implementation |
| gSOAP `wsseapi.c` (GPL, reference only) | Canonical WS-Security in a systems language |
| globusdigital/soap (Go, MIT) | Minimal SOAP dispatch pattern — `HandleOperation(action, tag, factory, handler)` |

## Rust Dependencies

| Crate | Purpose |
|-------|---------|
| `roxmltree` | WSDL/XSD parsing (read-only DOM, MIT, used at startup) |
| `quick-xml` | Per-request SOAP envelope parsing/serialization (streaming, fast) |
| `axum` | HTTP server framework |
| `tokio` | Async runtime |
| `sha1` | WS-Security digest computation |
| `base64` | WS-Security nonce encoding |
| `chrono` | WS-Security timestamp validation |

## Architecture

Three layers with clean separation:

### Layer 1: WSDL/XSD Parser

Parses WSDL 1.1 and XSD schemas at startup. Builds an in-memory representation of the service definition.

#### WSDL Parser

Implements the full WSDL 1.1 specification. The object model follows the canonical structure defined by JSR-110 (wsdl4j):

```rust
pub struct WsdlDefinition {
    pub target_namespace: String,
    pub imports: Vec<WsdlImport>,
    pub types: TypesSection,
    pub messages: HashMap<String, Message>,
    pub port_types: HashMap<String, PortType>,
    pub bindings: HashMap<String, Binding>,
    pub services: HashMap<String, Service>,
}

pub struct Message {
    pub name: String,
    pub parts: Vec<MessagePart>,
}

pub struct MessagePart {
    pub name: String,
    pub element: Option<QName>,    // for document style
    pub type_ref: Option<QName>,   // for RPC style
}

pub struct PortType {
    pub name: String,
    pub operations: Vec<Operation>,
}

pub struct Operation {
    pub name: String,
    pub input: Option<OperationMessage>,
    pub output: Option<OperationMessage>,
    pub faults: Vec<OperationFault>,
    pub style: OperationStyle,  // OneWay, RequestResponse, Solicit, Notification
}

pub struct OperationMessage {
    pub name: Option<String>,
    pub message: QName,
}

pub struct Binding {
    pub name: String,
    pub port_type: QName,
    pub soap_binding: SoapBinding,
    pub operations: Vec<BindingOperation>,
}

pub struct SoapBinding {
    pub style: BindingStyle,        // Document or RPC
    pub transport: String,          // typically http://schemas.xmlsoap.org/soap/http
    pub soap_version: SoapVersion,  // Soap11 or Soap12
}

pub struct BindingOperation {
    pub name: String,
    pub soap_action: String,
    pub input: BindingMessage,
    pub output: BindingMessage,
}

pub struct BindingMessage {
    pub body: SoapBody,
    pub headers: Vec<SoapHeader>,
}

pub struct SoapBody {
    pub use_attr: UseStyle,  // Literal or Encoded
    pub namespace: Option<String>,
    pub encoding_style: Option<String>,
}

pub struct Service {
    pub name: String,
    pub ports: Vec<Port>,
}

pub struct Port {
    pub name: String,
    pub binding: QName,
    pub address: String,
}
```

**Parsing approach** (ported from zeep's two-pass pattern):

1. **Parse pass:** Read WSDL XML via `roxmltree`. Walk the DOM, constructing the struct hierarchy. Each WSDL element type gets a `parse(node: &roxmltree::Node) -> Result<Self>` function. These are pure functions — XML node in, struct out.

2. **Import resolution:** Before resolving references, recursively load all `<wsdl:import>` and `<xsd:import>` / `<xsd:include>` targets. Cache by namespace/location to handle diamond imports and prevent cycles.

3. **Resolve pass:** Wire cross-references — message refs in operations point to actual Message structs, binding port_type refs point to actual PortType structs, etc. This handles forward references without needing a symbol table during parsing.

#### XSD Schema Parser

Full XSD schema support, ported from zeep's `xsd/` module. This is the largest component.

```rust
pub struct XsdSchema {
    pub target_namespace: Option<String>,
    pub elements: HashMap<QName, XsdElement>,
    pub types: HashMap<QName, XsdType>,
    pub attribute_groups: HashMap<QName, AttributeGroup>,
    pub groups: HashMap<QName, Group>,
}

pub enum XsdType {
    Complex(ComplexType),
    Simple(SimpleType),
}

pub struct ComplexType {
    pub name: Option<String>,
    pub content: ComplexContent,
    pub attributes: Vec<XsdAttribute>,
}

pub enum ComplexContent {
    Sequence(Vec<XsdElement>),
    All(Vec<XsdElement>),
    Choice(Vec<XsdElement>),
    Empty,
    SimpleContent(SimpleContentDef),
    ComplexExtension { base: QName, content: Box<ComplexContent> },
    ComplexRestriction { base: QName, content: Box<ComplexContent> },
}

pub struct SimpleType {
    pub name: Option<String>,
    pub restriction: Option<Restriction>,
    pub list: Option<ListDef>,
    pub union: Option<UnionDef>,
}

pub struct Restriction {
    pub base: QName,
    pub enumeration: Vec<String>,
    pub min_inclusive: Option<String>,
    pub max_inclusive: Option<String>,
    pub min_exclusive: Option<String>,
    pub max_exclusive: Option<String>,
    pub min_length: Option<u64>,
    pub max_length: Option<u64>,
    pub length: Option<u64>,
    pub pattern: Option<String>,
    pub whitespace: Option<WhitespaceHandling>,
    pub total_digits: Option<u64>,
    pub fraction_digits: Option<u64>,
}

pub struct XsdElement {
    pub name: Option<String>,
    pub type_ref: Option<QName>,
    pub inline_type: Option<XsdType>,
    pub min_occurs: u64,          // default 1
    pub max_occurs: MaxOccurs,    // Bounded(u64) or Unbounded
    pub nillable: bool,
    pub default: Option<String>,
    pub fixed: Option<String>,
    pub ref_attr: Option<QName>,  // <xs:element ref="..."/>
}

pub struct XsdAttribute {
    pub name: Option<String>,
    pub type_ref: Option<QName>,
    pub use_attr: AttributeUse,   // Required, Optional, Prohibited
    pub default: Option<String>,
    pub fixed: Option<String>,
    pub ref_attr: Option<QName>,
}
```

**XSD features to support:**

| Feature | Priority | Notes |
|---------|----------|-------|
| `xs:element` (global and local) | Critical | Top-level message wrappers |
| `xs:complexType` | Critical | All structured types |
| `xs:simpleType` | Critical | Enumerations, restrictions |
| `xs:sequence`, `xs:all`, `xs:choice` | Critical | Content model |
| `xs:extension` / `xs:restriction` | Critical | Type inheritance |
| `xs:import` / `xs:include` | Critical | Cross-schema references |
| `xs:attribute` / `xs:attributeGroup` | High | Used in SOAP headers |
| `xs:group` | High | Reusable content groups |
| `xs:any` / `xs:anyAttribute` | High | Extensibility points |
| `xs:list` / `xs:union` | Medium | Compound simple types |
| `xs:annotation` / `xs:documentation` | Low | Human-readable docs |
| `xs:key` / `xs:keyref` / `xs:unique` | Low | Rarely used in SOAP |
| `xs:notation` | Low | Essentially unused |

### Layer 2: SOAP Transport

Handles HTTP request/response, SOAP envelope parsing, dispatch, and serialization.

#### SOAP Envelope Processing

Supports both SOAP 1.1 and SOAP 1.2:

| Aspect | SOAP 1.1 | SOAP 1.2 |
|--------|----------|----------|
| Namespace | `http://schemas.xmlsoap.org/soap/envelope/` | `http://www.w3.org/2003/05/soap-envelope` |
| Content-Type | `text/xml` | `application/soap+xml` |
| Action location | `SOAPAction` HTTP header | `action` param in Content-Type |
| Fault structure | `faultcode` + `faultstring` | `Code/Value` + `Reason/Text` |

#### Request Processing Pipeline

```
HTTP POST received
    |
    +-- Extract Content-Type -> determine SOAP version (1.1 or 1.2)
    |
    +-- Parse XML body via quick-xml
    |   +-- Extract <soap:Header> children
    |   +-- Extract <soap:Body> first child element
    |
    +-- WS-Security check (if configured)
    |   +-- Extract wsse:Security from Header
    |   +-- Validate UsernameToken
    |   +-- Reject with SOAP Fault on failure
    |
    +-- Determine operation:
    |   +-- Document/literal: Body child element local name is the dispatch key
    |   +-- RPC style: SOAPAction header is the dispatch key
    |   +-- Fallback: match SOAPAction header
    |
    +-- Look up handler in dispatch table
    |   +-- No handler found -> SOAP Fault (action not supported)
    |
    +-- Deserialize request body into handler's expected type
    |   +-- Deserialization failure -> SOAP Fault (malformed request)
    |
    +-- Call handler
    |   +-- Handler returns Ok(response) -> serialize to XML
    |   +-- Handler returns Err(fault) -> serialize SOAP Fault
    |
    +-- Wrap response in SOAP Envelope
        - Success: HTTP 200
        - SOAP 1.1 Fault: HTTP 500
        - SOAP 1.2 Fault: HTTP 500 (per W3C SOAP 1.2 spec, Section 7.4.2)
```

#### Dispatch Table

Built at startup from the parsed WSDL:

```rust
struct DispatchTable {
    /// Document/literal: body element QName -> operation
    by_element: HashMap<QName, Arc<OperationDef>>,
    /// SOAPAction header value -> operation
    by_action: HashMap<String, Arc<OperationDef>>,
}

struct OperationDef {
    name: String,
    soap_action: String,
    input_element: QName,
    output_element: QName,
    handler: Box<dyn OperationHandler>,
}
```

#### SOAP Fault Generation

```rust
pub struct SoapFault {
    pub code: FaultCode,
    pub reason: String,
    pub detail: Option<String>,
    pub node: Option<String>,
    pub role: Option<String>,
}

pub enum FaultCode {
    VersionMismatch,
    MustUnderstand,
    DataEncodingUnknown,
    Sender,    // SOAP 1.2; maps to "Client" in 1.1
    Receiver,  // SOAP 1.2; maps to "Server" in 1.1
    Custom(String),
}
```

Faults serialize differently for SOAP 1.1 vs 1.2. The transport layer handles this automatically based on the binding's SOAP version.

#### WSDL Serving

`GET /path?wsdl` returns the WSDL XML with the `soap:address location` rewritten to match the server's actual address. Imported XSD schemas are either inlined or served at their own URLs.

### Layer 3: WS-Security

Implements the OASIS WS-Security UsernameToken profile.

#### UsernameToken Digest Validation

```
Expected digest = Base64(SHA-1(Base64Decode(Nonce) + Created + Password))
```

Steps:
1. Extract `wsse:Security` header from SOAP Header
2. Extract `wsse:UsernameToken` containing:
   - `wsse:Username` — plaintext username
   - `wsse:Password Type="...#PasswordDigest"` — the digest value
   - `wsse:Nonce EncodingType="...#Base64Binary"` — base64-encoded random nonce
   - `wsu:Created` — ISO 8601 timestamp
3. Decode nonce from base64
4. Concatenate: `decoded_nonce_bytes + created_utf8_bytes + password_utf8_bytes`
5. Compute SHA-1 hash
6. Base64-encode the hash
7. Compare against the provided Password value

#### PasswordText Validation

Simpler: the Password element contains the plaintext password. Compare directly.

#### Timestamp Validation

The `wsu:Created` timestamp is checked against the server's clock. Configurable tolerance (default: 300 seconds / 5 minutes) to handle clock skew between client and server.

#### Nonce Replay Prevention

Optional feature. Maintain a time-windowed cache of recently seen nonces. Reject any request that reuses a nonce within the window. Default window: 300 seconds.

#### Auth Bypass

The server supports marking specific operations as auth-exempt. This is used by consumers like ONVIF where certain operations (e.g., `GetSystemDateAndTime`) must be accessible without authentication so clients can synchronize clocks before computing digests.

## Public API

```rust
use soap_server::{Server, Wsdl, Auth, SoapFault};

// Load WSDL
let wsdl = Wsdl::from_file("path/to/service.wsdl").await?;

// Build server
let server = Server::from_wsdl(wsdl)
    .handler("OperationName", |req: MyRequest| async move {
        Ok(MyResponse { ... })
    })
    .handler("AnotherOp", another_handler)
    .auth(Auth::wsse_digest("username", "password"))
    .auth_exempt("GetSystemDateAndTime")
    .build()?;

// Get an axum Router to compose with other routes
let router = server.into_router();

// Or run standalone
axum::serve(listener, router).await?;
```

### Handler Trait

```rust
#[async_trait]
pub trait OperationHandler: Send + Sync + 'static {
    /// The XML element name this handler responds to
    fn operation_name(&self) -> &str;

    /// Handle the operation. Receives raw XML bytes of the request body element.
    /// Returns raw XML bytes of the response body element.
    /// Returning Err produces a SOAP Fault.
    async fn handle(&self, request: &[u8]) -> Result<Vec<u8>, SoapFault>;
}
```

Higher-level typed handlers are provided via a generic adapter that uses `yaserde` or `quick-xml` for deserialization/serialization:

```rust
// Users can register closures that work with typed structs:
server.typed_handler::<GetStatusRequest, GetStatusResponse>("GetStatus", |req| async move {
    Ok(GetStatusResponse { ... })
})
```

## Crate Structure

```
soap-server/
    src/
        lib.rs              # Public API, re-exports
        server.rs           # Server builder, axum integration
        dispatch.rs         # Dispatch table construction and lookup
        envelope.rs         # SOAP envelope parsing and serialization
        fault.rs            # SOAP Fault types and serialization
        security.rs         # WS-Security UsernameToken
        wsdl/
            mod.rs          # WSDL loader, import resolution
            parse.rs        # WSDL XML -> struct parsing functions
            definitions.rs  # WSDL data structures
        xsd/
            mod.rs          # XSD schema loader
            parse.rs        # XSD XML -> struct parsing functions
            types.rs        # XSD type definitions (ComplexType, SimpleType, etc.)
            elements.rs     # XSD element definitions
            visitor.rs      # Schema traversal utilities
    tests/
        wsdl_parsing.rs     # Test against real-world WSDLs
        soap_dispatch.rs    # Request routing tests
        security.rs         # WS-Security validation tests
        envelope.rs         # Envelope parsing/serialization tests
        integration.rs      # End-to-end: load WSDL, register handler, send request
    examples/
        simple_service.rs   # Minimal SOAP service example
```

## Testing Strategy

- Parse real-world WSDLs (ONVIF, common public SOAP services) and verify the struct graph
- Round-trip tests: parse WSDL -> build dispatch table -> send SOAP request -> verify handler called with correct data -> verify response envelope
- WS-Security: test PasswordDigest and PasswordText against known test vectors from the OASIS specification
- SOAP 1.1 and 1.2 envelope parsing/serialization with known-good XML samples
- Fault generation for both SOAP versions

## Build Priority

For the first consumer (onvif-server), these features are needed first:

1. WSDL 1.1 parser — full spec
2. XSD schema parser — full spec
3. SOAP 1.2 envelope parsing and serialization
4. Document/literal binding dispatch
5. WS-Security UsernameToken (PasswordDigest + PasswordText)
6. WSDL serving on `GET ?wsdl`
7. SOAP fault generation (SOAP 1.2)
8. axum integration

Then backfill:
- SOAP 1.1 envelope support
- RPC/encoded binding style
- SOAP 1.1 fault format
- MTOM/XOP support
- Multiple services per WSDL
- Comprehensive test suite against public SOAP WSDLs

## Context

soap-server is the foundation layer of a two-crate stack. It must be a correct, general-purpose SOAP server — not ONVIF-specific. The ONVIF-specific layer lives in onvif-server.

Dependency chain: `soap-server` <- `onvif-server`

Both crates are published to crates.io under NavistAu ownership.
