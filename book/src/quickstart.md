# Quick Start

A complete, runnable service is in the repository as the
[`simple_service`](https://github.com/NavistAu/soap-server/blob/main/examples/simple_service.rs)
example with its WSDL fixture
[`hello.wsdl`](https://github.com/NavistAu/soap-server/blob/main/examples/hello.wsdl).
Run it with `cargo run --example simple_service`. This page walks that example
from WSDL operation to wire response.

## The contract: one operation in the WSDL

`hello.wsdl` declares a single document/literal operation. The pieces that matter
for wiring a handler are the operation name and its input/output element names:

```xml
<xs:element name="SayHello">                 <!-- request body element -->
  <xs:complexType><xs:sequence>
    <xs:element name="Name" type="xs:string" minOccurs="1"/>
  </xs:sequence></xs:complexType>
</xs:element>
<xs:element name="SayHelloResponse">         <!-- response body element -->
  <xs:complexType><xs:sequence>
    <xs:element name="Greeting" type="xs:string" minOccurs="1"/>
  </xs:sequence></xs:complexType>
</xs:element>
...
<wsdl:operation name="SayHello"> ... </wsdl:operation>
```

## The server

```rust,no_run
use soap_server::{escape_text, FnHandler, ServerBuilder, SoapFault};
use bytes::Bytes;

#[tokio::main]
async fn main() {
    let svc = ServerBuilder::from_wsdl_file("examples/hello.wsdl")
        // "SayHello" MUST match the <wsdl:operation name="..."> above, or
        // .build() returns Err — misnamed handlers fail at startup.
        .handler(
            "SayHello",
            FnHandler::new(|body: Bytes| async move {
                // `body` is the <SayHello> element. Parse out <Name> (see the
                // example for the quick-xml read_text + unescape helper)...
                let name = parse_name(&body).unwrap_or_else(|| "world".into());
                // ...then return the <SayHelloResponse> element. The library wraps
                // it in the SOAP envelope verbatim, so escape any text you inject.
                let xml = format!(
                    r#"<SayHelloResponse xmlns="urn:example:hello"><Greeting>Hello, {}!</Greeting></SayHelloResponse>"#,
                    escape_text(&name)
                );
                Ok::<Bytes, SoapFault>(Bytes::from(xml))
            }),
        )
        .path("/hello")
        .build()
        .expect("WSDL build failed");

    let router = svc.into_router();
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    axum::serve(listener, router).await.unwrap();
}
# fn parse_name(_b: &bytes::Bytes) -> Option<String> { None }
```

## Try it

Send the `SayHello` request (SOAP 1.2 — the version is auto-detected):

```sh
curl -s http://localhost:8080/hello \
  -H 'Content-Type: application/soap+xml' \
  -d '<s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope">
        <s:Body>
          <SayHello xmlns="urn:example:hello"><Name>Ada</Name></SayHello>
        </s:Body>
      </s:Envelope>'
```

You get back your `SayHelloResponse` element, wrapped in a SOAP 1.2 envelope by
the library:

```xml
<env:Envelope xmlns:env="http://www.w3.org/2003/05/soap-envelope"><env:Body>
  <SayHelloResponse xmlns="urn:example:hello"><Greeting>Hello, Ada!</Greeting></SayHelloResponse>
</env:Body></env:Envelope>
```

Fetch the contract itself with a GET — every service path also serves its WSDL,
with the `<soap:address>` rewritten to the request URL:

```sh
curl -s 'http://localhost:8080/hello?wsdl'
```

## Step-by-step

### 1. `ServerBuilder::from_wsdl_file`

`ServerBuilder::from_wsdl_file("path/to/service.wsdl")` loads and parses the WSDL at the
given filesystem path. The builder also accepts raw WSDL bytes or a custom `WsdlLoader`
implementation for resolving imports over other transports.

### 2. `.handler("MyOperation", ...)`

`.handler` registers an async operation handler. The first argument is the operation name
exactly as it appears in the WSDL `<operation>` element. Registering a name that is not in
the WSDL causes `.build()` to return an error.

The second argument is any value that implements `SoapHandler`. The `FnHandler` wrapper
converts a closure `Fn(Bytes) -> Future<Output = Result<Bytes, SoapFault>>` into a
`SoapHandler` without you needing to implement the trait manually.

The `Bytes` argument your closure receives is the SOAP Body's first child element as
self-contained XML — all ancestor namespace declarations are re-emitted on the fragment
root, so you can parse it independently.

Return `Ok(Bytes)` containing the response body element XML (without an enclosing envelope
— the library adds the envelope), or return `Err(SoapFault)` to send a SOAP Fault.

### 3. `.build()`

Parses the WSDL, validates that every registered operation name exists, builds the dispatch
table, and returns a `Result<SoapService, BuildError>`. Fail fast: call `.expect` or handle
the error at startup.

### 4. `svc.into_router()`

Converts the built `SoapService` into an `axum::Router` mounted at the URL derived from
the WSDL `<service><port address>` element. The router is a standard axum `Router` and can
be nested or merged with other routers.

### 5. `axum::serve`

Standard axum server startup. The library has no opinion on TLS termination, timeouts, or
other middleware — compose those layers on top of the returned `Router` before calling
`axum::serve`.

## Implementing `SoapHandler` directly

For handlers that need access to SOAP header fragments (e.g. WS-Addressing), implement the
`SoapHandler` trait directly and override `handle_with_headers`:

```rust,no_run
use soap_server::{SoapHandler, SoapFault};
use bytes::Bytes;
use async_trait::async_trait;

struct MyHandler;

#[async_trait]
impl SoapHandler for MyHandler {
    async fn handle(&self, body: Bytes) -> Result<Bytes, SoapFault> {
        Ok(Bytes::from(r#"<MyResponse xmlns="urn:example"/>"#))
    }

    async fn handle_with_headers(
        &self,
        body: Bytes,
        headers: &[Bytes],
    ) -> Result<Bytes, SoapFault> {
        // Each element of `headers` is the raw bytes of one direct child of <Header>.
        let _ = headers; // inspect as needed
        self.handle(body).await
    }
}
```
