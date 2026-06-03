# Architecture

## Overview

`soap-server` is a pipeline of discrete stages that run for every incoming HTTP request.
Each stage is implemented in its own module. The library never generates code at compile
time (no proc-macros); all WSDL analysis happens at server startup inside `.build()`.

```
HTTP request
    │
    ▼
[envelope]   parse_envelope — detect SOAP version, extract header children + body element
    │
    ▼
[dispatch]   DispatchTable::lookup — route body first-child QName to a DispatchEntry
    │
    ▼
[dispatch]   validate_request — XSD structural check against TypeRegistry
    │
    ▼
[server]     WS-Security check (if auth enabled and operation not in bypass set)
    │
    ▼
[handler]    SoapHandler::handle_with_headers — your application logic
    │
    ▼
[envelope]   serialize_envelope — wrap response bytes in a SOAP envelope
    │
    ▼
HTTP response
```

## Modules

### `dispatch`

Builds and holds the `DispatchTable` — a pair of `HashMap` keyed on body-element `QName`
(primary) and `SOAPAction` string (fallback). The table is built once from a `ResolvedWsdl`
during `.build()` and never mutated per-request, so lookups are O(1) and lock-free.

Each `DispatchEntry` carries the `Arc<dyn SoapHandler>`, an `auth_required` flag, the
`input_type` QName used for routing, and a `validation_type` QName used for XSD structural
validation. Both may be `None` for operations with empty or omitted input elements.

The build step rejects any operation name that was registered via `.handler()` but does not
appear in the WSDL — this is the mechanism that makes unregistered-operation detection a
build-time error rather than a runtime panic.

### `server`

`ServerBuilder` and `SoapService` live here. `ServerBuilder` accumulates the WSDL source,
handler map, auth closure, bypass set, and other configuration. `.build()` resolves the
WSDL (including any imported XSDs), constructs the `DispatchTable` and `TypeRegistry`, and
returns a `SoapService`.

`SoapService::into_router()` mounts the service's HTTP handler at the URL derived from the
WSDL `<service><port address>` element and returns a plain `axum::Router`. Merging multiple
such routers is how multi-WSDL / multi-service deployments are composed:

```rust,no_run
# use soap_server::ServerBuilder;
# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let svc_a = ServerBuilder::from_wsdl_file("a.wsdl").build()?.into_router();
let svc_b = ServerBuilder::from_wsdl_file("b.wsdl").build()?.into_router();

// Each router mounts at its own path; merge them into one axum app.
let app = svc_a.merge(svc_b);
let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
axum::serve(listener, app).await?;
# Ok(())
# }
```

### `handler`

Defines the `SoapHandler` trait and the `FnHandler` convenience wrapper.

`SoapHandler` has two methods:

- `handle(body: Bytes) -> Result<Bytes, SoapFault>` — receives the body element fragment,
  returns response XML bytes.
- `handle_with_headers(body: Bytes, headers: &[Bytes]) -> Result<Bytes, SoapFault>` —
  default implementation calls `handle`, ignoring header fragments. Override this to
  inspect WS-Addressing or other header-level protocols.

`FnHandler<F>` wraps any `Fn(Bytes) -> impl Future<Output = Result<Bytes, SoapFault>>`
into a `SoapHandler`.

### `fault`

`SoapFault` carries a `FaultCode`, a human-readable `reason` string, an optional plain-text
`detail` string, and an optional `detail_xml` field for pre-formed XML detail content.

`FaultCode` variants map to both SOAP 1.1 and 1.2 fault code strings:

| `FaultCode` variant    | SOAP 1.2           | SOAP 1.1        |
|------------------------|--------------------|-----------------|
| `VersionMismatch`      | `env:VersionMismatch` | `env:VersionMismatch` |
| `MustUnderstand`       | `env:MustUnderstand`  | `env:MustUnderstand`  |
| `DataEncodingUnknown`  | `env:DataEncodingUnknown` | `env:Server`    |
| `Sender`               | `env:Sender`       | `env:Client`    |
| `Receiver`             | `env:Receiver`     | `env:Server`    |

The `detail_xml` field takes precedence over `detail` when both are set, and its content is
emitted verbatim (not escaped) into the fault detail element.

### `envelope`

`parse_envelope` and `serialize_envelope` handle the SOAP envelope layer for both protocol
versions. `detect_soap_version` infers the version from the `Content-Type` header
(`text/xml` → SOAP 1.1, `application/soap+xml` → SOAP 1.2). The parsed `ParsedEnvelope`
exposes the SOAP version, a `Vec<Bytes>` of header child fragments, and the body first-child
element as self-contained XML bytes (with ancestor namespace declarations re-emitted on the
fragment root).

### `qname`

`QName` is a lightweight `(Option<String>, String)` tuple representing an XML qualified
name. It is the key type for `DispatchTable`'s element-based lookup map. `QName::new`
creates a namespaced name; `QName::local` creates a no-namespace name.

### `xml_escape`

`escape_text` and `escape_attr` are thin wrappers over `quick_xml::escape::escape` for
escaping all five XML special characters. They are separate functions to communicate intent
at call sites and to allow future divergence (e.g. skipping `"` in text-only contexts).
Both functions are exported at the crate root.
