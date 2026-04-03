# Stack Research

**Domain:** Rust library crate — SOAP server (XML parsing, HTTP, cryptography)
**Researched:** 2026-04-03
**Confidence:** HIGH (all versions verified against crates.io API)

## Recommended Stack

### Core Technologies

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| roxmltree | 0.21.1 | WSDL/XSD DOM parsing at startup | Read-only tree API is ideal for two-pass parse+resolve of WSDL documents; fastest DOM library in Rust; zero-copy where possible; actively maintained by RazrFalcon |
| quick-xml | 0.39.2 | Per-request SOAP envelope streaming parse/write | SAX-style streaming avoids heap allocation on the hot path; 10x faster than serde-xml-rs; supports both reading and writing; handles namespaces correctly |
| axum | 0.8.8 | HTTP server framework and Router integration | Dominant Rust web framework (18M+ downloads); composes via `Router::merge` so consumers can add their own routes; built on tower so middleware is standard |
| tokio | 1.50.0 | Async runtime | De facto standard Rust async runtime; required by axum; `tokio::sync::Mutex` for nonce cache |

### Supporting Libraries

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| sha1 (RustCrypto) | 0.11.0 | WS-Security PasswordDigest — `Base64(SHA-1(nonce + created + password))` | Always — WS-Security UsernameToken spec mandates SHA-1 for PasswordDigest |
| hmac (RustCrypto) | 0.13.0 | HMAC-SHA1 if needed for token validation extensions | If extending beyond bare PasswordDigest |
| base64 | 0.22.1 | Encode/decode nonce and digest in WS-Security headers | Always — nonce and PasswordDigest are Base64-encoded in SOAP headers |
| chrono | 0.4.44 | Parse and validate WS-Security `<Created>` timestamps; freshness window enforcement | Always — nonce replay protection requires timestamp comparison |
| uuid | 1.23.0 | Generate unique nonces for WS-Security nonce cache keys | Always — nonce must be cryptographically unique per request |
| thiserror | 2.0.18 | Derive `Error` for `SoapFault` and internal error types | Always — idiomatic error handling for a library crate |
| bytes | 1.11.1 | `Bytes` type for zero-copy body passing through axum handler | Always — axum body extraction returns `Bytes`; avoids unnecessary copies |
| tower | 0.5.3 | `Service` trait — lets consumers wrap the SOAP router in tower middleware | Optional — only if exposing a tower `Service` impl in the public API |
| tower-http | 0.6.8 | `TraceLayer`, `CorsLayer`, `CompressionLayer` — optional middleware consumers might want | Optional — document as a suggested consumer dependency, not a direct dep |
| http-body-util | 0.1.3 | `BodyExt::collect` for buffering request body in axum extractors | Always — required pattern for reading raw bytes in axum 0.7+ |

### Development Tools

| Tool | Purpose | Notes |
|------|---------|-------|
| cargo test | Unit and integration tests | Test SOAP envelope parsing, WSDL dispatch, and WS-Security digest computation in isolation |
| cargo clippy | Lint enforcement | Run with `--all-targets --all-features -D warnings` in CI |
| cargo fmt | Code style | `rustfmt.toml` with `edition = "2021"` |
| cargo doc | Doc generation | All public types need `///` doc comments; verify with `cargo doc --no-deps --open` |
| insta | Snapshot testing for XML round-trips | Useful for envelope serialization and fault generation tests |

## Installation

```toml
# Cargo.toml — library dependencies
[dependencies]
roxmltree   = "0.21"
quick-xml   = { version = "0.39", features = ["async-tokio"] }
axum        = "0.8"
tokio       = { version = "1", features = ["sync"] }
sha1        = "0.11"
base64      = "0.22"
chrono      = { version = "0.4", features = ["std"], default-features = false }
uuid        = { version = "1", features = ["v4"] }
thiserror   = "2"
bytes       = "1"
http-body-util = "0.1"

[dev-dependencies]
tokio       = { version = "1", features = ["full", "test-util"] }
axum        = { version = "0.8", features = ["http2"] }
```

## Alternatives Considered

| Recommended | Alternative | When to Use Alternative |
|-------------|-------------|-------------------------|
| roxmltree | minidom | If you need mutable DOM manipulation — roxmltree is read-only, which is a feature not a bug for WSDL parsing |
| roxmltree | xmltree | Never for this use case — xmltree is slower and less ergonomic than roxmltree |
| quick-xml | serde-xml-rs | If you want serde derive macros on structs — acceptable for very simple envelopes, but 10x slower and loses namespace control; not appropriate for SOAP |
| quick-xml | xml-rs | xml-rs is older, slower, and unmaintained compared to quick-xml |
| axum | actix-web | If the consumer's application is already actix-based — axum is the standard choice for greenfield; actix-web doesn't compose as cleanly via router |
| axum | warp | Never — warp is effectively unmaintained as of 2024 |
| chrono | time | Either works for timestamp arithmetic; chrono has broader adoption in the Rust ecosystem and simpler API for parsing ISO 8601 strings |
| sha1 (RustCrypto) | ring | ring is acceptable but has a C build step; RustCrypto/sha1 is pure Rust, easier to audit, and sufficient for WS-Security PasswordDigest |
| uuid v4 | random nonce bytes | Both work for nonce generation; uuid is more idiomatic and has a well-understood replay-resistance story |

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| soap-service crate | Only 1,415 total downloads; opinionated macro-based API that forces structure; 0.2.1 as of May 2025 with minimal adoption — we are building what this should be | Build from scratch with roxmltree + quick-xml + axum |
| wsdl crate | Last updated July 2023 (0.1.3); 19 recent downloads; incomplete spec coverage; uses roxmltree underneath but doesn't expose the two-pass resolve pattern needed | Implement WSDL parsing directly on roxmltree |
| savon / soafe | Client-only code generators; generate structs from WSDL at build time, not runtime dispatch servers | Not applicable — we need runtime WSDL dispatch |
| yaserde | Opinionated XML-serde mapping with known limitations on complex XSD; zeep-rs depends on it but it causes codegen artifacts | quick-xml for runtime; direct tree traversal for WSDL |
| serde-xml-rs | 10x slower than quick-xml; incomplete namespace handling; not maintained to the same standard | quick-xml |
| hyper (direct) | Axum already wraps hyper correctly; direct hyper use adds boilerplate without benefit | axum |
| openssl crate | Requires system libssl; adds non-Rust build dependency; unnecessary for SHA-1 digest operations | sha1 from RustCrypto (pure Rust) |

## Stack Patterns by Variant

**For startup WSDL/XSD parsing (one-time, cold path):**
- Use roxmltree for full DOM traversal and forward-reference resolution
- Two-pass: parse into intermediate structs, then resolve `$ref` and `$include` chains
- Load WSDL as `&str` into `roxmltree::Document`, traverse into owned data structures

**For per-request SOAP envelope handling (hot path):**
- Use quick-xml streaming reader — never allocate a full DOM per request
- Parse `<Envelope>/<Header>/<Body>` with a state machine; stop at `<Body>` child element name to do dispatch lookup
- Write response envelopes with quick-xml `Writer` directly into a `Vec<u8>`

**For WS-Security PasswordDigest verification:**
- Compute: `Base64::encode(SHA-1(base64_decode(nonce) + created_bytes + password_bytes))`
- Store seen `(nonce, created)` pairs in a `tokio::sync::Mutex<HashMap<String, Instant>>`
- Evict entries older than the freshness window (300 seconds per WS-Security spec)

**For axum Router integration:**
- Expose `SoapService::into_router() -> axum::Router` — consumer calls `app.merge(soap.into_router())`
- Register a single POST handler for the service endpoint path
- Register a GET handler on the same path for `?wsdl` serving

## Version Compatibility

| Package | Compatible With | Notes |
|---------|-----------------|-------|
| axum 0.8.x | tokio 1.x, tower 0.5.x, hyper 1.x | axum 0.8 requires hyper 1 — do not mix with hyper 0.14 |
| quick-xml 0.39.x | No axum/tokio coupling | Pure parsing library; async-tokio feature adds `AsyncReader` if needed |
| roxmltree 0.21.x | No runtime coupling | Startup-only; no async needed |
| sha1 0.11.x | hmac 0.13.x (RustCrypto family) | sha1 0.11 uses digest 0.11 trait; hmac 0.13 is compatible |
| chrono 0.4.44 | Stable; no known breaking changes with above stack | Use `default-features = false` to avoid `time` crate conflicts |
| thiserror 2.x | Rust 1.68+ | thiserror 2 is a breaking change from 1.x — do not mix in the same dep tree |

## Existing Rust SOAP Ecosystem Assessment

The Rust SOAP ecosystem is extremely thin. No production-grade SOAP *server* crate exists on crates.io as of April 2026. The only server-adjacent crate (`soap-service`, 0.2.1) has 1,415 total downloads and is a thin macro wrapper. All other SOAP crates are code-generation clients (savon, soafe, zeep-rs) or stubs.

This confirms the greenfield nature of the project: there is no prior art to build on top of, and no risk of collision with an established crate.

## Sources

- crates.io API (`/api/v1/crates/<name>`) — version verification for all 12 crates listed (HIGH confidence)
- [roxmltree GitHub](https://github.com/RazrFalcon/roxmltree) — DOM vs streaming tradeoff analysis (HIGH confidence)
- [quick-xml GitHub](https://github.com/tafia/quick-xml) — performance characteristics (HIGH confidence)
- [axum docs.rs](https://docs.rs/axum/latest/axum/) — Router composition and body extraction patterns (HIGH confidence)
- [WS-Security UsernameToken Profile 1.1](https://docs.oasis-open.org/wss/v1.1/wss-v1.1-spec-os-UsernameTokenProfile.pdf) — PasswordDigest formula (HIGH confidence)
- WebSearch: Rust SOAP server ecosystem 2025 — confirmed no production server crate exists (HIGH confidence — absence of results is itself the finding)
- WebSearch: Rust XML library comparison — confirmed roxmltree performance and API characteristics (MEDIUM confidence — verified against official README)

---
*Stack research for: soap-server — general-purpose SOAP server crate for Rust*
*Researched: 2026-04-03*
