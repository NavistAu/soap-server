# soap-server — Round 1 Consolidated Review
Date: 2026-04-21
Sources: round-1-soap-server-claude.md, round-1-soap-server-codex.md
Reviewers: Claude (sonnet), OpenAI Codex (gpt-5.3-codex v0.122.0)

## Blockers for Plan 05-05

### [BLOCK-SS-C01] Two `.unwrap()` calls in non-test server.rs build path
Source: [BLOCK-SS-01 from Claude]
File: src/server.rs:196, src/server.rs:215
Description: `resolved.definition.services.get(svc_name).unwrap()` in the multi-service dispatch table builder. The invariant holds in practice (svc_name was just collected from services.values()) but is not type-enforced. Any `.unwrap()` in library code is a publish-readiness red flag.
Severity: Release-blocker (library code panics are unacceptable)
Fix: Replace with `ok_or_else(|| BuildError::UnknownService(svc_name.clone()))?` — add `BuildError::UnknownService(String)` variant.

### [BLOCK-SS-C02] Nonce cache idle-gap bug — valid requests rejected after low-traffic periods
Source: [BLOCK-SS-CDX-01 from Codex]
File: src/wssec/nonce_cache.rs:37-39
Description: `rotate_if_needed()` only rotates once per check. If the server is idle for 2+ half_window periods, nonces in `previous` that should have been evicted remain, causing legitimate clients to get "replay detected" faults. Reproduction: insert nonce, wait 2× half_window_secs (or mock time in test), send again — should accept but rejects.
Severity: Production correctness blocker — ONVIF devices on low-traffic networks will fail intermittently.
Fix: Loop rotation: `while self.bucket_start.elapsed().as_secs() >= self.half_window_secs { self.previous = std::mem::take(&mut self.current); self.bucket_start += Duration::from_secs(self.half_window_secs); }` — or equivalently clear both buckets if 2+ windows elapsed.

### [BLOCK-SS-C03] xs:choice elements incorrectly treated as required in schema validation
Source: [BLOCK-SS-CDX-03 from Codex]
File: src/dispatch.rs:360-361
Description: `collect_required_from_content` treats `ComplexContent::Choice` the same as `Sequence` and `All`. For ONVIF SOAP operations that use `xs:choice` in their input type (select exactly one of N elements), the validation step rejects valid requests — all `minOccurs>0` children of the choice are treated as mandatory simultaneously, but a legal request only provides one.
Severity: ONVIF interoperability blocker — any operation with a choice group will always fail validation before reaching the handler.
Fix: For `ComplexContent::Choice`, apply `minOccurs > 0` only to the choice group as a whole (at least one branch present), not to each individual element within the choice. Simplest correct approach: return empty required list for Choice (no individual element is independently required; the choice is required as a group, which is harder to validate structurally).

### [BLOCK-SS-C04] SOAP version inconsistency between envelope and Content-Type in responses
Source: [BLOCK-SS-CDX-02 from Codex]
File: src/server.rs:612-613
Description: Response is serialized with `soap_version` from Content-Type header but the response Content-Type is set from `envelope.soap_version` parsed from the XML envelope namespace. If a client sends mismatched SOAP 1.1/1.2 signals (header says one version, envelope namespace says another), the response envelope and response Content-Type disagree. This breaks SOAP spec compliance.
Severity: Interoperability blocker for misbehaving-but-real clients.
Fix: Determine version from a single authoritative source (envelope namespace is more reliable than Content-Type in practice for ONVIF); use that version for both serialization and Content-Type response.

### [BLOCK-SS-C05] `compute_digest` exported with ambiguous `&[u8]` nonce parameter encoding
Source: [BLOCK-SS-02 from Claude]
File: src/lib.rs:19, src/wssec/username_token.rs
Description: `compute_digest` is a public API but its `nonce` parameter is expected to be base64-decoded bytes — this is not documented. A consumer passing a raw nonce string (not decoded) silently computes a wrong digest with no error.
Severity: API surface blocker — silently wrong cryptographic output is a security concern.
Fix: Option A: add rustdoc with exact encoding contract. Option B: take `base64_nonce: &str` and decode internally, return `Result<Vec<u8>, DecodeError>`. Option B is safer. Option C: `#[doc(hidden)]` + remove from public API if intended only for onvif-server internal use.

### [BLOCK-SS-C06] `RotatingNonceCache` thread-safety contract undocumented
Source: [BLOCK-SS-03 from Claude]
File: src/lib.rs:18, src/wssec/nonce_cache.rs
Description: `RotatingNonceCache::check_and_insert` takes `&mut self` — must be wrapped in a Mutex for concurrent use. The server internals do this correctly, but consumers using the exported function `validate_username_token` need this documented.
Severity: Security API documentation blocker — unclear contract around shared state.
Fix: Document in rustdoc: "Wrap in `tokio::sync::Mutex` for use in async handlers." Consider interior mutability design for v0.2.

### [BLOCK-SS-C07] Missing crate-level `//!` documentation block
Source: [BLOCK-SS-04 from Claude]
File: src/lib.rs
Description: lib.rs has no `//!` module-level documentation. docs.rs will render a blank crate root page with no explanation of what the crate does, how to get started, or what features exist.
Severity: Publish-readiness blocker — a crate with no docs.rs root documentation will not be adopted.
Fix: Add `//!` block explaining: what the crate is, SOAP 1.1/1.2 support, WSDL-driven dispatch, WS-Security, minimum usage snippet. (Plan 05-07 scope but classified blocker here.)

### [BLOCK-SS-C08] `FaultCode::as_str()` returns SOAP 1.2 codes only — misleading for SOAP 1.1 consumers
Source: [BLOCK-SS-05 from Claude]
File: src/fault.rs:14-23
Description: `FaultCode::as_str()` returns `"env:Sender"` for `Sender`, `"env:Receiver"` for `Receiver`. These are SOAP 1.2 codes. SOAP 1.1 uses `"env:Client"` and `"env:Server"`. The actual XML serialization is correct (uses the right names per version), but the public `as_str()` method returns the wrong values if a consumer is building SOAP 1.1 responses.
Severity: Public API naming blocker — ambiguous method returning version-specific values without documentation.
Fix: Rename to `as_soap12_str()` and add `as_soap11_str()`, or add a SOAP-1.2-explicit note in the rustdoc.

## Non-blockers (document as known limitations)

- [NB-SS-C01] docs.yml has duplicate `on.push` YAML keys — main branch push won't trigger docs workflow [from Codex NB-SS-CDX-01]
  File: .github/workflows/docs.yml:4-7
  Recommendation: Merge the two push triggers into one YAML mapping block (one `push:` key with both `branches` and `tags` sub-keys).

- [NB-SS-C02] WSDL address rewriting always uses http:// — breaks HTTPS/reverse-proxy deployments [from Codex NB-SS-CDX-02]
  File: src/server.rs:763
  Recommendation: Read `X-Forwarded-Proto` header to determine scheme. Document limitation in README for 0.1.0.

- [NB-SS-C03] `DispatchError` and `BuildError` are separate types without a `From` impl [from Claude NB-SS-01]
  Recommendation: Add `From<DispatchError> for BuildError` for `?` propagation ergonomics.

- [NB-SS-C04] `ServerBuilder::new()` is private — no discoverable entry point in rustdoc [from Claude NB-SS-02]
  Recommendation: Document that `from_wsdl_file`, `from_wsdl_bytes`, `from_wsdl_bytes_with_loader` are the three entry points.

- [NB-SS-C05] `DispatchEntry` fields are all `pub` — advanced API without documentation [from Claude NB-SS-03]
  Recommendation: Document that `DispatchEntry` is an advanced API. Consider `pub(crate)` for `handler` field.

- [NB-SS-C06] `SoapFault` fields are `pub` — consumers can construct arbitrary faults [from Claude NB-SS-04]
  Recommendation: Document this is intentional (useful for destructuring in match arms).

- [NB-SS-C07] `WsdlLoader::load()` lacks async contract and error semantics documentation [from Claude NB-SS-05]
  Recommendation: Add rustdoc on `load()` explaining `None` vs `Err` semantics.

- [NB-SS-C08] No optional feature flags — WS-Security deps always compiled [from Claude NB-SS-06]
  Recommendation: Consider `wssec` feature flag for v0.2+. Non-blocker at 0.1.0.

- [NB-SS-C09] Cargo.toml description says "SOAP 1.2" but crate supports both 1.1 and 1.2 [from Claude NB-SS-07]
  Recommendation: Update description to mention both versions.

- [NB-SS-C10] `SoapService` (output of `build()`) has no rustdoc [from Claude NB-SS-08]
  Recommendation: Add rustdoc with usage pattern.

- [NB-SS-C11] No `examples/` directory [from Claude NB-SS-09]
  Recommendation: Planned for plan 05-07.

- [NB-SS-C12] `build_dispatch_table_for_service` not re-exported from lib.rs [from Claude NB-SS-10]
  Recommendation: Add re-export to lib.rs if intended as public API.

## Reviewer Agreements (high confidence — both Claude and Codex flagged)

Both reviewers identified:
- nonce cache correctness issues (Claude flagged the thread-safety angle; Codex found the idle-gap rotation bug)
- SOAP version handling inconsistencies
- Documentation gaps

## Decisions Required

1. **`compute_digest` API shape (BLOCK-SS-C05):** Keep as `&[u8]` with docs, change signature to take base64 string and decode, or make it `#[doc(hidden)]`? This decision affects the public API surface and is semver-relevant. Recommended: make `#[doc(hidden)]` if only used internally by onvif-server; otherwise fix signature.

2. **`FaultCode::as_str()` rename (BLOCK-SS-C08):** Rename to `as_soap12_str()` + add `as_soap11_str()` (breaking change if any consumer already uses `as_str()`), or just add docs? At 0.1.0, renaming is safe since no published consumers exist. Recommended: rename.

3. **docs.yml GH Pages configuration (NB-SS-C01):** Fix the YAML duplicate key now (trivial fix) or defer to plan 05-07. Recommended: fix now in plan 05-05 since it's a one-line YAML fix.

## Already Planned (skip — do not list as blockers)

- No README → plan 05-07
- Cargo.toml metadata (repository, keywords, categories, readme, documentation, homepage) → plan 05-08
- `path = "../soap-server"` dep swap → plan 05-10 (affects onvif-server not soap-server)
- Missing examples/ → plan 05-07

## Summary
Blockers: 8 | Non-blockers: 12 | Decisions: 3

**Blocker breakdown:**
- 2 blockers from Codex only (nonce idle-gap bug CDX-01, xs:choice validation CDX-03)
- 1 blocker from both reviewers (SOAP version mismatch)
- 5 blockers from Claude only (server.rs unwrap, compute_digest API, nonce thread-safety docs, missing lib.rs docs, FaultCode::as_str naming)
