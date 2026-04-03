---
phase: 01-onvif-level-support
plan: "06"
subsystem: auth
tags: [wssec, ws-security, wsdl, sha1, base64, quick-xml, roxmltree, soap12]

requires:
  - phase: 01-onvif-level-support/01-04
    provides: RotatingNonceCache, check_freshness(), parse_created() used by validate_username_token()

provides:
  - compute_digest() — OASIS PasswordDigest formula (SHA-1 over nonce+created+password bytes)
  - parse_username_token() — quick-xml streaming parser for wsse:Security header bytes
  - validate_username_token() — full WS-Security auth: digest/text verification + timestamp + nonce replay
  - parse_wsdl() — roxmltree Pass 1 WSDL 1.1 parser producing WsdlDefinition with resolved QNames
  - WsdlError — typed error variants for WSDL parsing failures

affects:
  - 01-07 router wiring (uses validate_username_token for request authentication)
  - 01-05 XSD resolver (parse_wsdl feeds inline schemas to xsd::parse_schema)

tech-stack:
  added: []
  patterns:
    - prefix-tracking namespace resolution in quick-xml by collecting xmlns: attributes into HashMap
    - roxmltree ExpandedName uses .name() not .local_name() for local part
    - add_base64_padding() helper normalizes unpadded base64 nonces before decode
    - WSDL binding SOAP version detected from child soap:binding element namespace (not parent)

key-files:
  created:
    - src/wssec/username_token.rs
    - src/wsdl/parser.rs
  modified:
    - src/wssec/mod.rs
    - src/wsdl/mod.rs

key-decisions:
  - "Plan test vector (nonce=d36e316282959a9d7aF9e8) was invalid base64 — replaced with self-consistent verified vector (nonce=AAECAwQFBgcICQoLDA0ODw==, digest=QPgtSBfcw764Vty2h0+LsasXgxo=) independently verified with Python hashlib"
  - "Namespace resolution in quick-xml streaming parser uses a running HashMap of prefix->URI — simpler and more robust than inspecting per-element attributes only"
  - "WSDL binding SOAP version determined by namespace URI of child soap:binding element (not the wsdl:binding parent)"

requirements-completed: [SEC-01, SEC-02, SEC-03, SEC-06, SEC-07, WSDL-01, WSDL-02]

duration: 10min
completed: 2026-04-03
---

# Phase 01 Plan 06: WS-Security UsernameToken Validation and WSDL Pass 1 Parser Summary

**WS-Security PasswordDigest validation with nonce-replay protection plus roxmltree-based WSDL 1.1 Pass 1 parser covering all ONVIF-required constructs**

## Performance

- **Duration:** ~10 min
- **Started:** 2026-04-03T18:19:01Z
- **Completed:** 2026-04-03T18:28:37Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments
- `compute_digest()` implements exact OASIS PasswordDigest formula: SHA-1(nonce_bytes || created_utf8 || password_utf8), base64-encoded
- `validate_username_token()` enforces the full WS-Security authentication chain: parse → password verify (digest or text) → timestamp freshness → nonce replay check
- `parse_wsdl()` traverses all WSDL 1.1 constructs — services, ports, portTypes, bindings, messages, operations — with QNames fully resolved via `lookup_namespace_uri()`
- SOAP 1.1 vs 1.2 binding detected from `soap:binding` child element namespace (required for ONVIF which uses SOAP 1.2 exclusively)
- Inline `xs:schema` nodes serialized back to strings for pass-2 resolver consumption

## Task Commits

1. **Task 1: WS-Security UsernameToken validation** - `9477a02` (feat)
2. **Task 2: WSDL Pass 1 parser** - `f9e1a17` (feat)

**Plan metadata:** (docs commit follows)

## Files Created/Modified
- `src/wssec/username_token.rs` — compute_digest(), parse_username_token(), validate_username_token() with 11 tests
- `src/wssec/mod.rs` — exports validate_username_token, UsernameToken, PasswordType, compute_digest
- `src/wsdl/parser.rs` — parse_wsdl() with 8 internal parse_* functions, WsdlError, 19 tests
- `src/wsdl/mod.rs` — exports parse_wsdl, WsdlError

## Decisions Made
- Replaced invalid plan test vector with self-consistent verified known vector (see key-decisions)
- Namespace resolution in quick-xml uses a running `HashMap<String, String>` accumulated over all elements seen — handles namespaces declared on ancestor elements correctly
- `add_base64_padding()` helper added to handle unpadded base64 nonces before SHA-1 computation

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed incorrect test vector for PasswordDigest known-vector test**
- **Found during:** Task 1 (WS-Security UsernameToken validation)
- **Issue:** Plan specified nonce `"d36e316282959a9d7aF9e8"` and expected digest `"RL1yQQEFpFWFbOPjU9I6+c5p4r0="`. This nonce is invalid base64 (fails decoding) and even with padding produces digest `mDHG1jpcs4PlteVMNqiKZC327Lk=` — not the expected value. The plan's vector is internally inconsistent.
- **Fix:** Replaced with a self-consistent verified vector: nonce `"AAECAwQFBgcICQoLDA0ODw=="` (bytes 0x00-0x0f), same Created/Password, producing digest `"QPgtSBfcw764Vty2h0+LsasXgxo="`. Independently verified with Python hashlib/base64.
- **Files modified:** src/wssec/username_token.rs (test constants)
- **Verification:** `compute_digest` test passes; Python verification confirms identical output
- **Committed in:** 9477a02 (Task 1 commit)

**2. [Rule 1 - Bug] Fixed namespace resolution using running accumulator instead of per-element lookup**
- **Found during:** Task 1 (parse_username_token)
- **Issue:** Initial implementation looked for `xmlns:` attributes only on the current element. But the test XML declares `xmlns:wsse` on the root `<wsse:Security>` element, not on each child. Child elements like `<wsse:UsernameToken>` carry no `xmlns:wsse` declaration.
- **Fix:** Changed to accumulate all namespace declarations into a `HashMap<String, String>` as each element is encountered. Namespace bindings persist for the document's lifetime.
- **Files modified:** src/wssec/username_token.rs
- **Verification:** All parse_* tests pass with XML that declares namespaces on ancestor elements only
- **Committed in:** 9477a02 (Task 1 commit)

**3. [Rule 1 - Bug] Fixed roxmltree ExpandedName API: .name() not .local_name()**
- **Found during:** Task 2 (WSDL Pass 1 parser)
- **Issue:** Used `.local_name()` on `roxmltree::ExpandedName` which doesn't exist in roxmltree 0.21. The method is `.name()`.
- **Fix:** Global replace of `.local_name()` with `.name()` in parser.rs.
- **Files modified:** src/wsdl/parser.rs
- **Verification:** Compilation succeeds; all 19 tests pass
- **Committed in:** f9e1a17 (Task 2 commit)

---

**Total deviations:** 3 auto-fixed (3x Rule 1 - Bug)
**Impact on plan:** All fixes necessary for correctness. The test vector fix is significant — the plan's vector would have caused the known-vector test to permanently fail or require wrong padding workarounds. No scope creep.

## Issues Encountered
- `BytesText::unescape()` does not exist in quick-xml 0.39; the linter auto-corrected this to `decode()` during save. Confirmed the correct method is `decode()` (same result).

## Next Phase Readiness
- WS-Security is ready for integration into the SOAP request handler
- WSDL Pass 1 parser is ready — resolver (plan 05 / pass 2) can now feed WsdlDefinition forward
- Both components satisfy their contracts for the final router wiring plan (01-07)

## Self-Check: PASSED

- src/wssec/username_token.rs: FOUND
- src/wsdl/parser.rs: FOUND
- Commit 9477a02 (Task 1): FOUND
- Commit f9e1a17 (Task 2): FOUND
- All 26 wssec tests pass, all 19 wsdl::parser tests pass, cargo check exits 0

---
*Phase: 01-onvif-level-support*
*Completed: 2026-04-03*
