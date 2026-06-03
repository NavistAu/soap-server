# soap-server

A WSDL-driven SOAP 1.1/1.2 server library for Rust, built on top of [axum](https://docs.rs/axum).

Provide a WSDL file, register async handler closures for each operation, and get a fully
spec-compliant SOAP endpoint with no boilerplate envelope handling.

[![crates.io](https://img.shields.io/crates/v/soap-server.svg)](https://crates.io/crates/soap-server)
[![docs.rs](https://img.shields.io/docsrs/soap-server)](https://docs.rs/soap-server)
[![license](https://img.shields.io/crates/l/soap-server.svg)](https://github.com/NavistAu/soap-server#license)

---

## Features

- **SOAP 1.1 and 1.2** — auto-detects version from the `Content-Type` header and envelope
  namespace; responds in the same version as the incoming request.
- **WSDL-driven dispatch** — operations are discovered from the WSDL at server build time.
  Registering a handler for an operation name that does not exist in the WSDL causes
  `.build()` to return `Err` — no runtime panics on misnamed operations.
- **WS-Security (UsernameToken)** — supports `PasswordDigest` and `PasswordText`
  authentication with nonce replay detection and timestamp freshness checks.
- **XSD structural validation** — required elements in the request body are validated against
  the WSDL/XSD schema before the handler is called.

---

## Installation

```sh
cargo add soap-server
```

Or add manually to `Cargo.toml`:

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
soap-server = "0.1.0"
```

**MSRV:** Rust **1.88.0** or later.

---

## Quick Start

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

- `ServerBuilder::from_wsdl_file` loads and parses the WSDL. The builder also accepts raw
  bytes or a custom `WsdlLoader` implementation.
- `.handler("MyOperation", ...)` registers an async handler. The `Bytes` your closure
  receives is the SOAP Body's first child element as self-contained XML (ancestor namespace
  declarations are re-emitted on the fragment root). Return `Ok(Bytes)` with the response
  body element (no enclosing envelope needed) or `Err(SoapFault)`.
- `.build()` validates all registered operation names against the WSDL and returns
  `Result<SoapService, BuildError>`.
- `svc.into_router()` returns an `axum::Router` mounted at the URL from the WSDL
  `<service><port address>` element.

---

## WS-Security

Call `.auth(...)` on the builder to require `UsernameToken` authentication on all
operations:

```rust,no_run
use soap_server::{FnHandler, ServerBuilder, SoapFault};
use bytes::Bytes;

#[tokio::main]
async fn main() {
    let svc = ServerBuilder::from_wsdl_file("path/to/service.wsdl")
        .auth(|username: &str| -> Option<String> {
            match username {
                "admin" => Some("s3cr3t".to_string()),
                _ => None,
            }
        })
        .auth_bypass(["GetSystemDateAndTime"])
        .handler(
            "MyOperation",
            FnHandler::new(|_body: Bytes| async move {
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

- The `.auth` closure receives the username from `<wsse:UsernameToken>` and returns the
  expected plaintext password (`None` to deny). Digest comparison is performed internally
  with constant-time equality.
- Both `PasswordText` and `PasswordDigest` (`Base64(SHA-1(nonce + created + password))`)
  are accepted.
- Nonce replay detection uses a rotating in-memory cache with a default window of 300 s.
  Timestamp freshness is enforced to ±300 s.
- `.auth_bypass(["..."])` exempts named operations from the security header requirement
  (useful for clock-sync or discovery operations).
- Operations with a missing or invalid `<wsse:Security>` header receive a `Sender` fault.

---

## Multi-WSDL / Multi-Service

Build each service separately and merge the resulting `axum::Router` instances. Each router
mounts at its own path derived from the respective WSDL:

```rust,no_run
use soap_server::ServerBuilder;

async fn example() -> Result<(), Box<dyn std::error::Error>> {
    let svc_a = ServerBuilder::from_wsdl_file("a.wsdl").build()?.into_router();
    let svc_b = ServerBuilder::from_wsdl_file("b.wsdl").build()?.into_router();

    let app = svc_a.merge(svc_b);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    axum::serve(listener, app).await?;
    Ok(())
}
```

---

## Documentation

- **API reference:** <https://docs.rs/soap-server>
- **User guide (mdBook):** <https://navistau.github.io/soap-server/>

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

---

## License

Licensed under either of:

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for
inclusion in this crate by you shall be dual-licensed as above, without any additional
terms or conditions.

Copyright Joshua Hogendorn / NavistAu.
