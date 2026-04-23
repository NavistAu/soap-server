# soap-server — Round 1 Review (Claude)
Date: 2026-04-21
Reviewer: Claude (sonnet)

## Blockers (must fix before 0.1.0 publish)

- [BLOCK-SS-01] Two `.unwrap()` calls in non-test library code in `server.rs`
  File: src/server.rs:196, src/server.rs:215
  Context: Inside `SoapService::build()` — the code calls `resolved.definition.services.get(svc_name).unwrap()` after a first pass that populates `service_names`. Because `svc_name` was collected from `services.values()` just above, the second `.get()` cannot fail in practice — but the panic path is reachable if the `services` map is mutated between passes (which is impossible with the current `&self` API, but the invariant is not enforced by types). The real concern is that any library code with a user-visible panic path blocks publication per the library crate standard.
  Impact: A panic in a library call is a soundness concern for consumers. Not a data-safety panic, but sets a bad precedent and would fail most publish review checklists.
  Suggested fix: Replace with `ok_or_else(|| BuildError::UnknownService(svc_name.clone()))?` and add `BuildError::UnknownService(String)` variant. The error is unreachable in practice but the type system confirms it.

- [BLOCK-SS-02] `compute_digest` is exported but takes raw `&[u8]` nonce + &str created + &str password with no type safety
  File: src/lib.rs:19, src/wssec/username_token.rs
  Impact: `compute_digest` is a low-level WS-Security crypto function. It is exported as part of the public API (re-exported in lib.rs) but its signature takes raw bytes with no documentation of expected encoding. The nonce is expected to be base64-decoded bytes, but this is not stated anywhere in the public API. A consumer calling it with a raw nonce string (not decoded) will compute a wrong digest silently — no error returned.
  Suggested fix: Either (a) add rustdoc specifying exact parameter encoding, or (b) change signature to take `base64_nonce: &str` and decode internally, surfacing a decode error. Given this is a crypto primitive, option (b) is safer. Alternatively, mark it `#[doc(hidden)]` and remove it from the public API if it is only intended for onvif-server internal use.

- [BLOCK-SS-03] `validate_username_token` is exported but the `nonce_cache` parameter requires a `&mut RotatingNonceCache` — not thread-safe without external `Mutex` wrapping
  File: src/lib.rs:18-19, src/wssec/username_token.rs
  Impact: `validate_username_token` and `RotatingNonceCache` are both re-exported. A consumer who tries to use `validate_username_token` in an async handler (which is `Send + Sync`) must wrap `RotatingNonceCache` in a `Mutex`. This is not documented. The server internals use `Mutex<RotatingNonceCache>` correctly, but a consumer using the exported function directly has no guidance. Worse: if `RotatingNonceCache` is stored in an `Arc` without a mutex and used from multiple async tasks, the `&mut self` on `check_and_insert` prevents this at compile time — but the overall API design forces a footgun pattern that isn't documented.
  Suggested fix: Document the thread-safety contract clearly: "Wrap in `tokio::sync::Mutex` for use in async handlers." Alternatively, consider making the cache interior-mutable (`Arc<Mutex<...>>` internally) so `check_and_insert` takes `&self`. This is the safer design for a public API.

- [BLOCK-SS-04] Missing `#![deny(missing_docs)]` or at minimum `#![warn(missing_docs)]` — no crate-level documentation
  File: src/lib.rs
  Impact: `lib.rs` has no crate-level documentation (`//!` block). All re-exported items lack docs at the crate root level. `SoapHandler`, `FnHandler`, `DispatchTable`, `DispatchError`, `BuildError` all have some inline docs but none of the module-level re-exports are documented from the crate root. `cargo doc --workspace --no-deps` produces no "missing documentation" warnings because `#![warn(missing_docs)]` is not enabled — but docs.rs will render a near-empty crate root page, which is confusing for consumers.
  Suggested fix: Add `//!` crate-level doc block to lib.rs. Add `#![warn(missing_docs)]` (plan 05-07 scope, but the absence is a blocker for publish — users cannot find API guidance on docs.rs without it).

- [BLOCK-SS-05] `SoapFault` is missing `Display` impl documentation — `FaultCode::as_str()` returns SOAP 1.2 codes, but SOAP 1.1 codes are different
  File: src/fault.rs:14-23
  Impact: `FaultCode::as_str()` returns `"env:Sender"`, `"env:Receiver"`, etc. (SOAP 1.2 QName-style codes). For SOAP 1.1 these should be `"env:Client"` and `"env:Server"`. The SOAP 1.1 fault serialization path (`to_xml_bytes_v11`) uses a hardcoded match for `FaultCode` (correctly), so the actual XML output is correct. However, `as_str()` is a public method that returns wrong values for the SOAP 1.1 use case if called by a consumer. This is a public API surface issue.
  Suggested fix: Either rename to `as_soap12_str()` and add `as_soap11_str()`, or document the SOAP 1.2 nature of the returned string explicitly in the rustdoc.

## Non-blockers (should fix / document known limitations)

- [NB-SS-01] `DispatchError` and `BuildError` are separate types but represent similar startup failures — no unified error type
  File: src/dispatch.rs:53, src/server.rs (BuildError)
  Recommendation: Document that `BuildError` wraps `DispatchError` variants in its own enum. Consider `From<DispatchError> for BuildError` to allow `?` propagation.

- [NB-SS-02] `ServerBuilder::new()` is private — the only entry points are `from_wsdl_file`, `from_wsdl_bytes`, `from_wsdl_bytes_with_loader`
  File: src/server.rs:54-68
  Recommendation: This is intentional and good API design (avoids unconfigured builders). Document in the rustdoc on `ServerBuilder` that these are the three entry points. Without docs a consumer may search for `ServerBuilder::new()` and fail silently.

- [NB-SS-03] `DispatchEntry` is `pub` but has `pub` fields — internal routing struct exposed without documentation
  File: src/dispatch.rs:14-20
  Recommendation: `DispatchEntry` is reachable via `pub mod dispatch` which is re-exported. Its fields `handler`, `auth_required`, and `input_type` are all public. This may be intentional for advanced users, but `handler: Arc<dyn SoapHandler>` being public means users could call handlers directly, bypassing auth. Document that `DispatchEntry` is an advanced API and that `handler` should not be called directly. Or make the fields `pub(crate)` and expose read-only accessors.

- [NB-SS-04] `SoapFault` fields (`code`, `reason`, `detail`) are `pub` — consumers can construct arbitrary faults without using constructors
  File: src/fault.rs:27-31
  Recommendation: The public fields make `SoapFault` easy to destructure in match arms (which is useful). Document this is intentional. Currently there is no validation of `code`/`reason` content; this is acceptable for a SOAP library where consumers may legitimately construct custom faults.

- [NB-SS-05] `WsdlLoader` trait is exported but its `load` method is not documented with async contract or error semantics
  File: src/wsdl/resolver.rs (re-exported as `pub use crate::wsdl::resolver::WsdlLoader`)
  Recommendation: Add rustdoc to `WsdlLoader::load()` explaining the expected return type, what `None` means vs error, and when each case applies.

- [NB-SS-06] Feature flags: no features defined in Cargo.toml — the crate always pulls axum + tokio + all deps
  File: Cargo.toml
  Recommendation: Consider whether `wssec` (WS-Security support with sha1/base64/chrono) should be an optional feature. At 0.1.0 this is a non-blocker, but worth noting since some SOAP consumers don't use WS-Security.

- [NB-SS-07] `description` in Cargo.toml says "SOAP 1.2 server library" but the crate supports both SOAP 1.1 and 1.2
  File: Cargo.toml:5
  Recommendation: Update description to "A spec-compliant SOAP 1.1 and 1.2 server library for Rust with WSDL-driven dispatch". Minor but visible on crates.io.

- [NB-SS-08] `SoapService` returned from `build()` — its API (`into_router()`, etc.) is not documented
  File: src/server.rs
  Recommendation: `SoapService` is the primary output of the builder but has no rustdoc. Add docs showing the intended usage pattern.

- [NB-SS-09] No `examples/` directory — consumers have no runnable quickstart
  File: (missing)
  Recommendation: A minimal echo-service example is planned for plan 05-07. Note as known gap.

- [NB-SS-10] `build_dispatch_table_for_service` is `pub` but not re-exported from `lib.rs`
  File: src/dispatch.rs:87, src/lib.rs
  Recommendation: `build_dispatch_table` is re-exported (line 12 of lib.rs) but `build_dispatch_table_for_service` is not. If both are intended to be public API, add the re-export. If `build_dispatch_table_for_service` is intended for advanced users only, document as such.

## cargo publish --dry-run output

Not run (requires local cargo credentials; deferred to plan 05-08 `cargo publish --dry-run` verification). The following issues would likely occur if run now:
- Missing `repository` field in Cargo.toml (required for crates.io best practices)
- Missing `readme` field (no README.md exists)
- Missing `keywords` and `categories` fields
- Missing `homepage` and `documentation` fields
These are all planned for plan 05-08 metadata polish and plan 05-07 README creation.

## cargo doc warnings

Not run (would require a full build environment). Anticipated warnings based on source inspection:
- All `pub` items in `src/lib.rs` lack crate-level `//!` docs
- `WsdlLoader` trait doc (if any) does not document the `load` method contract
- `DispatchEntry`, `DispatchTable` lack comprehensive docs on their fields
- `SoapHandler` trait doc on `handle()` method is incomplete (no mention of what `body` contains)

## Summary
5 blockers, 10 non-blockers.

Blockers prioritized for plan 05-05:
1. BLOCK-SS-01: server.rs unwrap() — easy fix (add BuildError variant, one line change)
2. BLOCK-SS-02: compute_digest API ambiguity — requires doc or signature change
3. BLOCK-SS-03: RotatingNonceCache thread-safety documentation
4. BLOCK-SS-04: Missing crate-level docs (lib.rs `//!` block) — plan 05-07 but flag now
5. BLOCK-SS-05: FaultCode::as_str() SOAP 1.1 vs 1.2 naming issue — rename or document

Non-blockers are deferred to plan 05-07 (docs) or noted as known limitations.
