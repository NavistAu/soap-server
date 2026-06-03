# Quick Start

This example shows the minimum required to get a SOAP service listening on port 8080.

```rust,no_run
use soap_server::{FnHandler, ServerBuilder, SoapFault};
use bytes::Bytes;

#[tokio::main]
async fn main() {
    let svc = ServerBuilder::from_wsdl_file("path/to/service.wsdl")
        .handler(
            "MyOperation",
            FnHandler::new(|_body: Bytes| async move {
                // Parse body, call business logic, return response XML bytes.
                Ok::<Bytes, SoapFault>(Bytes::from(
                    r#"<MyOperationResponse xmlns="urn:example"/>"#,
                ))
            }),
        )
        .build()
        .expect("WSDL build failed");

    let router = svc.into_router();
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    axum::serve(listener, router).await.unwrap();
}
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
