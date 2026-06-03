# WS-Security

`soap-server` supports the WS-Security `UsernameToken` profile with both `PasswordDigest`
and `PasswordText` credential modes.

## Enabling authentication

Call `.auth(...)` on the `ServerBuilder` before `.build()`:

```rust,no_run
use soap_server::{FnHandler, ServerBuilder, SoapFault};
use bytes::Bytes;

#[tokio::main]
async fn main() {
    let svc = ServerBuilder::from_wsdl_file("path/to/service.wsdl")
        .auth(|username: &str| -> Option<String> {
            // Look up the expected password for the given username.
            // Return Some(password) to allow, None to deny.
            match username {
                "admin" => Some("s3cr3t".to_string()),
                _ => None,
            }
        })
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

The closure receives the username from the incoming `<wsse:UsernameToken>` and must return
the expected plaintext password (or `None` to reject). The library performs the digest
comparison internally.

## Exempting operations

Use `.auth_bypass([...])` to list operation names that do **not** require a
`<wsse:Security>` header. This is useful for clock-sync or discovery operations that must
be reachable unauthenticated:

```rust,no_run
# use soap_server::ServerBuilder;
ServerBuilder::from_wsdl_file("path/to/service.wsdl")
    .auth(|username: &str| Some("password".to_string()))
    .auth_bypass(["GetSystemDateAndTime"])
    // ...
# ;
```

## PasswordDigest and PasswordText

Both variants of `wsse:Password` are accepted:

- **PasswordText** — the `<wsse:Password>` element contains the plaintext password. The
  library compares it with constant-time equality against the value your closure returns.
- **PasswordDigest** — the `<wsse:Password>` element contains
  `Base64(SHA-1(nonce + created + password))`. The library recomputes the digest using the
  password your closure returns and compares with constant-time equality.

The `compute_digest` and `validate_username_token` helpers from `soap_server::wssec` are
also exported at the crate root if you need to implement custom token validation logic.

## Nonce replay and timestamp freshness

Every request with a `PasswordDigest` token is checked against a rotating in-memory nonce
cache. The cache window defaults to **300 seconds** (±150 s half-window). A nonce seen
within that window causes the request to be rejected with a `Sender` fault.

Timestamp freshness is enforced with a default tolerance of **±300 seconds**. Requests
whose `<wsu:Created>` timestamp falls outside that window are rejected.

The `RotatingNonceCache` type is exported publicly if you need to pass a pre-configured
cache instance to the builder for non-default window sizes.

## Authentication failure response

Operations that require authentication but receive a missing or invalid
`<wsse:Security>` header receive a `Sender` fault in the appropriate SOAP version.
