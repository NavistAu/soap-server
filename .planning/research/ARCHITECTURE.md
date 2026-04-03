# Architecture Research

**Domain:** SOAP server library (Rust crate)
**Researched:** 2026-04-03
**Confidence:** HIGH (zeep/node-soap source verified; SOAP spec verified; WS-Security spec verified)

## Standard Architecture

### System Overview

A SOAP server library has two distinct operational phases that must be kept separate: a startup phase that parses and resolves WSDL/XSD into an in-memory model, and a per-request phase that dispatches incoming XML to handler functions using that model. Mixing these phases (e.g., re-parsing WSDL on each request) is a common and severe mistake.

```
STARTUP PHASE (once)
┌─────────────────────────────────────────────────────────────────────┐
│  WSDL/XSD Files                                                      │
│       │                                                              │
│       ▼                                                              │
│  ┌──────────────┐   ┌──────────────┐   ┌────────────────────────┐  │
│  │  WSDL Parser │──▶│ XSD Schema   │──▶│  Service Model         │  │
│  │  (roxmltree) │   │ Resolver     │   │  (Service/Port/Op map) │  │
│  └──────────────┘   └──────────────┘   └────────────┬───────────┘  │
│                                                      │              │
│                                         ┌────────────▼───────────┐  │
│                                         │  Dispatch Table        │  │
│                                         │  (body-elem → handler) │  │
│                                         └────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘

PER-REQUEST PHASE (every HTTP POST)
┌─────────────────────────────────────────────────────────────────────┐
│  HTTP Layer (axum)                                                   │
│       │                                                              │
│       ▼                                                              │
│  ┌──────────────────┐                                               │
│  │  Envelope Parser │  (quick-xml streaming)                        │
│  │  Header + Body   │                                               │
│  └────────┬─────────┘                                               │
│           │                                                          │
│           ▼                                                          │
│  ┌──────────────────┐                                               │
│  │  WS-Security     │  (UsernameToken validation, nonce cache)       │
│  │  Interceptor     │                                               │
│  └────────┬─────────┘                                               │
│           │                                                          │
│           ▼                                                          │
│  ┌──────────────────┐                                               │
│  │  Dispatcher      │  (body first-child QName → operation lookup)  │
│  └────────┬─────────┘                                               │
│           │                                                          │
│           ▼                                                          │
│  ┌──────────────────┐        ┌──────────────────────────────────┐  │
│  │  Handler (trait) │◀──────▶│  Consumer handler fn             │  │
│  │  (XML in/out)    │        │  (bytes → bytes or SoapFault)    │  │
│  └────────┬─────────┘        └──────────────────────────────────┘  │
│           │                                                          │
│           ▼                                                          │
│  ┌──────────────────┐                                               │
│  │  Envelope Builder│  (wrap response in SOAP 1.1/1.2 envelope)     │
│  └────────┬─────────┘                                               │
│           │                                                          │
│           ▼                                                          │
│  HTTP Response (axum)                                                │
└─────────────────────────────────────────────────────────────────────┘

GET ?wsdl
┌───────────────────────────────────────────────────────────────────┐
│  WSDL Serving Handler                                              │
│  (rewrite soap:address location to request URL, return XML)        │
└───────────────────────────────────────────────────────────────────┘
```

### Component Responsibilities

The following table maps directly to source modules. Each component is a standalone unit testable in isolation.

| Component | Responsibility | Owns |
|-----------|----------------|------|
| `wsdl::parser` | Parse WSDL XML into raw AST nodes (pass 1) | Reads XML, emits intermediate structs |
| `wsdl::resolver` | Resolve forward refs, imports, inline schemas (pass 2) | Turns AST into validated ServiceModel |
| `xsd::parser` | Parse XSD `<schema>` blocks and imported XSD files | complexType, simpleType, element, attribute |
| `xsd::resolver` | Resolve type refs across schemas, build type registry | TypeRegistry keyed by QName |
| `model` | In-memory WSDL object graph: Service/Port/PortType/Binding/Operation/Message | Read-only after build; shared via Arc |
| `dispatch` | Map QName of SOAP body first child to operation + handler | HashMap built at startup from model |
| `envelope` | Parse SOAP envelope (Header + Body); serialize response envelope | Handles 1.1 and 1.2 |
| `fault` | Build SOAP fault responses (version-aware format) | SOAP 1.1 faultcode/faultstring vs 1.2 Code/Reason |
| `wssec` | Validate WS-Security UsernameToken: PasswordDigest/Text, timestamp freshness, nonce replay | In-memory nonce cache with expiry |
| `handler` | `SoapHandler` trait: raw bytes in, raw bytes or SoapFault out | Defined in public API |
| `router` | Build axum `Router` combining dispatch, wssec, WSDL serving | Public API entry point |

## Recommended Project Structure

```
src/
├── lib.rs                  # Public API: SoapRouter, SoapHandler, SoapFault
├── router.rs               # axum Router construction; wires all components
├── handler.rs              # SoapHandler trait definition
├── fault.rs                # SoapFault type; SOAP 1.1/1.2 fault serialization
├── envelope.rs             # SOAP envelope parse + serialize (quick-xml)
├── dispatch.rs             # Dispatch table: QName → (operation, handler)
├── wssec/
│   ├── mod.rs              # Public interface: validate_token()
│   ├── username_token.rs   # PasswordDigest/PasswordText validation
│   ├── nonce_cache.rs      # Replay prevention: time-bounded seen-nonce store
│   └── timestamp.rs        # Timestamp freshness check (5-minute window)
├── wsdl/
│   ├── mod.rs              # Public: parse_wsdl(bytes) -> ServiceModel
│   ├── parser.rs           # Pass 1: roxmltree DOM → raw WSDL nodes
│   ├── resolver.rs         # Pass 2: forward-ref resolution, import walking
│   └── definitions.rs      # WSDL object types: Service, Port, Binding, Operation, Message
├── xsd/
│   ├── mod.rs              # Public: parse_schema(nodes) -> TypeRegistry
│   ├── parser.rs           # Parse complexType/simpleType/element/attribute
│   ├── resolver.rs         # Resolve $ref, extension, restriction, imports
│   └── types.rs            # XsdType, XsdElement, TypeRegistry
└── model.rs                # ServiceModel: assembled view joining WSDL + XSD output
```

### Structure Rationale

- **`wsdl/` and `xsd/` are separate packages:** They have different grammars, different resolution rules, and XSD can be used standalone. python-zeep separates them; this is the right call.
- **Two-pass parse pattern in both:** Pass 1 reads XML and constructs raw structs with unresolved QName references. Pass 2 resolves all references. This handles forward references and circular imports correctly, as proven in zeep.
- **`dispatch.rs` is separate from `model.rs`:** The model knows about WSDL structure; dispatch knows about request routing. These are different concerns. Mixing them creates a god object.
- **`wssec/` is separate from `envelope.rs`:** Security validation is a middleware concern that should be independently testable and bypassable per-operation (e.g., GetSystemDateAndTime).
- **`router.rs` as the integration layer:** All axum wiring lives here. It takes a ServiceModel + handler map and produces an axum Router. Nothing else in the crate imports axum.

## Architectural Patterns

### Pattern 1: Two-Pass Parse (Parse + Resolve)

**What:** Pass 1 reads XML into intermediate structs with unresolved QName strings. Pass 2 walks the graph and replaces QName strings with concrete references to the target objects.

**When to use:** Any time a document format has forward references or imports (both WSDL and XSD do).

**Trade-offs:** More code than single-pass, but eliminates an entire class of ordering bugs. python-zeep uses this pattern; it is the correct approach for WSDL/XSD.

**Example:**
```rust
// Pass 1 output — unresolved
struct RawOperation {
    name: String,
    input_message: QName,   // "tns:GetProfilesRequest" — not yet resolved
    output_message: QName,
}

// Pass 2 output — resolved
struct Operation {
    name: String,
    input_message: Arc<Message>,  // concrete reference
    output_message: Arc<Message>,
}
```

### Pattern 2: Dispatch by Body First-Child QName

**What:** The dispatcher extracts the QName of the first child element of the SOAP Body, looks it up in a HashMap built at startup from the WSDL operation definitions, and routes to the matching handler.

**When to use:** Document/literal style (required for ONVIF and most modern SOAP). node-soap and WCF both use this pattern.

**Trade-offs:** O(1) dispatch; requires WSDL to be loaded. Falls back to SOAPAction header when body element is ambiguous (RPC style).

**Example:**
```rust
// Built at startup
let dispatch: HashMap<QName, Arc<dyn SoapHandler>> = build_dispatch_table(&model, &handlers);

// Per request
let body_first_child: QName = parse_body_first_child_qname(&body_bytes)?;
let handler = dispatch.get(&body_first_child)
    .ok_or_else(|| SoapFault::unknown_action(&body_first_child))?;
```

### Pattern 3: Security as Pre-Dispatch Interceptor

**What:** WS-Security validation runs before dispatch. Each operation carries a flag indicating whether auth is required. If required and validation fails, a SOAP fault is returned before the handler is ever called.

**When to use:** Any SOAP service with WS-Security. This is how all production implementations (CXF, WCF, zeep) structure it.

**Trade-offs:** Clean separation; handlers never see security headers. The bypass list (e.g., GetSystemDateAndTime) is checked before attempting validation, not inside the handler.

**Example:**
```rust
let requires_auth = model.operation_requires_auth(&operation_name);
if requires_auth {
    wssec::validate_token(&security_header, &nonce_cache)?;
}
// Only here do we dispatch to the handler
handler.call(body_bytes)
```

### Pattern 4: Arc<ServiceModel> Shared Across Requests

**What:** The WSDL/XSD model is parsed once at startup and stored as an `Arc<ServiceModel>`. axum clones the Arc for each request (cheap). No locking on the model itself (it's immutable after construction).

**When to use:** Any read-heavy shared state in async Rust. This is the standard axum pattern for shared state.

**Trade-offs:** Zero per-request allocation for model access. The only lock is in the nonce cache (write on every authenticated request).

## Data Flow

### Startup Data Flow

```
WSDL bytes (file or URL)
    │
    ▼
wsdl::parser::parse(bytes)          # roxmltree DOM traversal
    │ RawDefinition (unresolved refs)
    ▼
wsdl::resolver::resolve(raw_def)    # two-pass resolution
    │ + XSD schema blocks passed to xsd::parser
    ▼
xsd::parser::parse(schema_nodes)    # parse type definitions
    │ RawTypeGraph
    ▼
xsd::resolver::resolve(raw_types)   # resolve $ref, extension, restriction
    │ TypeRegistry
    ▼
model::build(definition, type_registry)  # assemble unified model
    │ ServiceModel (Arc)
    ▼
dispatch::build_table(model, handlers)   # QName → handler HashMap
    │ DispatchTable
    ▼
router::build(model, dispatch_table)     # axum Router
```

### Per-Request Data Flow

```
HTTP POST /soap
    │
    ▼
axum Handler (bytes extractor)
    │
    ▼
envelope::parse(bytes)              # quick-xml: split Header / Body
    │ (header_bytes, body_bytes)
    ▼
wssec::validate(header_bytes)       # if operation requires auth:
    │                               #   parse UsernameToken
    │                               #   check timestamp freshness (5 min window)
    │                               #   verify PasswordDigest: B64(SHA1(nonce+created+password))
    │                               #   check nonce not in replay cache
    │                               #   insert nonce into cache with TTL
    │ (or return SOAP fault)
    ▼
dispatch::route(body_bytes)         # extract first-child QName from Body
    │                               # HashMap lookup → handler
    │ SoapHandler reference
    ▼
handler.call(body_bytes)            # returns body XML bytes or SoapFault
    │
    ▼
envelope::serialize(response_body, soap_version)  # wrap in envelope
    │
    ▼
HTTP 200 with Content-Type: application/soap+xml (1.2) or text/xml (1.1)
```

### GET ?wsdl Data Flow

```
HTTP GET /soap?wsdl
    │
    ▼
wsdl_serving_handler
    │ read stored WSDL bytes
    │ rewrite soap:address location= to request Host + path
    ▼
HTTP 200 with Content-Type: text/xml
```

### SOAP Fault Data Flow

```
Any error at any phase (parse, auth, dispatch, handler)
    │
    ▼
fault::build(error, soap_version)
    │ SOAP 1.1: <faultcode> / <faultstring>
    │ SOAP 1.2: <Code><Value> / <Reason><Text>
    ▼
HTTP 500 with SOAP fault envelope
```

## Build Order (Phase Dependencies)

This is the dependency graph for implementation. Each layer cannot be tested without the layers below it being functional.

```
Layer 1 (no dependencies):
  - fault.rs          — pure struct + serialization, no external deps
  - xsd/types.rs      — data types only
  - wsdl/definitions.rs — data types only
  - handler.rs        — trait definition only

Layer 2 (depends on Layer 1):
  - envelope.rs       — depends on fault.rs for error path
  - xsd/parser.rs     — depends on xsd/types.rs
  - wsdl/parser.rs    — depends on wsdl/definitions.rs

Layer 3 (depends on Layer 2):
  - xsd/resolver.rs   — depends on xsd/parser.rs
  - wsdl/resolver.rs  — depends on wsdl/parser.rs, xsd/parser.rs

Layer 4 (depends on Layer 3):
  - model.rs          — depends on wsdl/resolver.rs, xsd/resolver.rs

Layer 5 (depends on Layer 4):
  - dispatch.rs       — depends on model.rs, handler.rs

Layer 6 (independent of model, depends on Layer 1):
  - wssec/timestamp.rs
  - wssec/nonce_cache.rs
  - wssec/username_token.rs  — depends on nonce_cache.rs, timestamp.rs

Layer 7 (integration, depends on all):
  - router.rs         — depends on dispatch.rs, envelope.rs, wssec/, fault.rs
  - lib.rs            — re-exports public API
```

**Recommended build sequence for phases:**

1. `fault.rs` + `envelope.rs` (bare minimum: parse envelopes and return faults)
2. `xsd/` (parser → resolver → types) — needed before WSDL can be validated
3. `wsdl/` (parser → resolver → definitions) — WSDL parsing milestone
4. `model.rs` + `dispatch.rs` — routing milestone
5. `wssec/` — security milestone
6. `router.rs` — integration milestone (first end-to-end test possible)

## Anti-Patterns

### Anti-Pattern 1: Parsing WSDL Per Request

**What people do:** Load and parse the WSDL file inside the request handler to get the operation definition.

**Why it's wrong:** WSDL parsing with roxmltree is DOM-based and allocates. At any meaningful load it becomes the bottleneck. The model is immutable after startup — there is no reason to re-parse it.

**Do this instead:** Parse once at startup, store as `Arc<ServiceModel>`, clone the Arc for axum state.

### Anti-Pattern 2: Single-Pass WSDL Parsing

**What people do:** Resolve type references as they are encountered during the first XML traversal.

**Why it's wrong:** WSDL and XSD have forward references. A type used in a message may be defined later in the same file or in an imported file. Single-pass resolution requires ordering or back-patching, both of which are fragile.

**Do this instead:** Two passes. Pass 1 collects raw nodes. Pass 2 resolves all references once the full document is loaded.

### Anti-Pattern 3: Dispatching on SOAPAction Header Alone

**What people do:** Route requests using only the `SOAPAction` HTTP header.

**Why it's wrong:** SOAPAction is optional in SOAP 1.2 and unreliable in practice. ONVIF devices and clients do not always send it. The correct primary dispatch key for document/literal is the QName of the SOAP Body's first child element.

**Do this instead:** Dispatch on body first-child QName. Fall back to SOAPAction if QName lookup fails (for RPC style compat).

### Anti-Pattern 4: Putting WS-Security Logic Inside Handlers

**What people do:** Each handler checks the security header itself and returns a fault if not authenticated.

**Why it's wrong:** Duplicated code across all handlers; auth bypass for specific operations (GetSystemDateAndTime) becomes scattered and inconsistent.

**Do this instead:** Security validation is a pre-dispatch interceptor. The dispatch table carries an `auth_required: bool` flag per operation. The interceptor runs before any handler is called.

### Anti-Pattern 5: Using DOM Parsing (roxmltree) Per Request

**What people do:** Use roxmltree for request envelope parsing because it's already a dependency for WSDL loading.

**Why it's wrong:** roxmltree builds a full owned DOM tree with allocations. For per-request parsing of SOAP envelopes that may arrive at high frequency, streaming with quick-xml is measurably faster and allocation-free.

**Do this instead:** roxmltree at startup for WSDL/XSD (correctness over speed). quick-xml streaming per request for envelope parsing (speed over convenience).

## Integration Points

### External Boundaries

| Boundary | Interface | Notes |
|----------|-----------|-------|
| axum Router | `SoapRouter::into_router()` returns `axum::Router` | Consumer merges with their own Router |
| Consumer handlers | `impl SoapHandler for MyHandler` | Raw XML bytes in/out; no axum types leak through |
| WSDL files | `SoapRouter::from_wsdl(path_or_bytes)` | Loaded once at startup |

### Internal Boundaries

| Boundary | Communication | Notes |
|----------|---------------|-------|
| `wsdl/` ↔ `xsd/` | `wsdl::resolver` calls `xsd::parser` for inline schema nodes | XSD parser takes `roxmltree::Node` slices |
| `model.rs` ↔ `dispatch.rs` | `dispatch::build_table` consumes `&ServiceModel` | Model is read-only after build |
| `router.rs` ↔ `wssec/` | `router.rs` calls `wssec::validate_token(header, &nonce_cache, config)` | Nonce cache is `Arc<Mutex<NonceCache>>` in axum state |
| `dispatch.rs` ↔ `handler.rs` | `dispatch.rs` holds `Arc<dyn SoapHandler>` per operation | Handler trait is the only cross-boundary type |

## Sources

- [zeep.wsdl internals — python-zeep documentation](https://docs.python-zeep.org/en/master/internals_wsdl.html) — WSDL Document/Definition hierarchy, binding separation
- [python-zeep wsdl/ source tree](https://github.com/mvantellingen/python-zeep/tree/master/src/zeep/wsdl) — Module structure reference
- [python-zeep wsdl/definitions.py](https://github.com/mvantellingen/python-zeep/blob/master/src/zeep/wsdl/definitions.py) — Service/Port/Binding/Operation/Message class relationships
- [python-zeep wsdl/bindings/soap.py](https://github.com/mvantellingen/python-zeep/blob/master/src/zeep/wsdl/bindings/soap.py) — Soap11Binding/Soap12Binding, fault format differences
- [node-soap server.ts](https://github.com/vpulim/node-soap/blob/master/src/server.ts) — Dispatch by body first-child QName algorithm
- [WS-Security UsernameToken Profile 1.1.1](https://docs.oasis-open.org/wss-m/wss/v1.1.1/os/wss-UsernameTokenProfile-v1.1.1-os.html) — PasswordDigest formula, nonce replay prevention, 5-minute timestamp window
- [Dispatch by Body Element — Microsoft WCF](https://learn.microsoft.com/en-us/dotnet/framework/wcf/samples/dispatch-by-body-element) — Document/literal dispatch pattern
- [gSOAP user guide](https://www.genivia.com/doc/guide/html/index.html) — Skeleton/dispatcher code generation model

---
*Architecture research for: Rust SOAP server crate (soap-server)*
*Researched: 2026-04-03*
