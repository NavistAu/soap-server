# Introduction

`soap-server` is a WSDL-driven SOAP 1.1/1.2 server library for Rust, built on top of
[axum](https://docs.rs/axum). You provide a WSDL file, register async handler closures
for each operation, and get a SOAP 1.1/1.2 endpoint with no boilerplate envelope
handling. It is a transport + dispatch layer — not a code generator or full XSD
validator; see [Capabilities & Limitations](./capabilities.md) for the precise scope.

## Features

- **SOAP 1.1 and 1.2** — auto-detects version from the `Content-Type` header and envelope
  namespace; responds in the same version as the incoming request.
- **WSDL-driven dispatch** — operations are discovered from the WSDL at server build time.
  Registering a handler for an operation name that does not exist in the WSDL is a build-time
  error (`.build()` returns `Err`).
- **WS-Security (UsernameToken)** — supports `PasswordDigest` and `PasswordText`
  authentication with nonce replay detection and timestamp freshness checks.
- **XSD structural validation** — required elements in the request body are validated against
  the WSDL/XSD schema before the handler is called.

## License

`soap-server` is dual-licensed under **MIT OR Apache-2.0**. You may choose either license.
