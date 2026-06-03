# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1] - 2026-06-03

### Changed

- Documentation: point the README "User guide" link at the live mdBook on GitHub Pages
  (<https://navistau.github.io/soap-server/>) instead of the "once the repo is public"
  placeholder; add the repository link to the book.

### Internal

- First release published via crates.io Trusted Publishing (OIDC) — validates the automated
  `release/* → main` publish pipeline (0.1.0 was a manual bootstrap publish).

## [0.1.0] - 2026-06-03

Initial release.

### Added

- **SOAP 1.1 and 1.2** support with automatic version detection from `Content-Type` header
  and envelope namespace; responses mirror the incoming request version.
- **WSDL-driven dispatch** via `ServerBuilder` — operations are discovered from the WSDL at
  server build time; registering an unknown operation name causes `.build()` to return `Err`.
- **WS-Security UsernameToken** authentication supporting both `PasswordDigest`
  (`Base64(SHA-1(nonce + created + password))`) and `PasswordText` credential modes.
  Includes nonce replay detection (rotating in-memory cache, 300 s window) and timestamp
  freshness enforcement (±300 s).
- **XSD structural validation** of required request body elements against the WSDL/XSD
  schema before the handler is invoked.
- `FnHandler` convenience wrapper for registering plain async closures as SOAP operation
  handlers without implementing `SoapHandler` directly.
- `SoapHandler` trait with `handle` and `handle_with_headers` methods for access to SOAP
  header fragments (e.g. WS-Addressing).
- Multi-WSDL / multi-service support by merging per-service `axum::Router` instances.
- `SoapFault` / `FaultCode` types covering all SOAP 1.1 and 1.2 fault codes.
- `RotatingNonceCache`, `compute_digest`, and `validate_username_token` exported at crate
  root for custom token validation logic.
- `escape_text` and `escape_attr` XML escaping helpers exported at crate root.

[Unreleased]: https://github.com/NavistAu/soap-server/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/NavistAu/soap-server/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/NavistAu/soap-server/releases/tag/v0.1.0
