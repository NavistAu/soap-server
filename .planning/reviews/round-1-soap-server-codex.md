# soap-server — Round 1 Review (Codex)
Date: 2026-04-21
Reviewer: OpenAI Codex (gpt-5.3-codex, v0.122.0)
Codex session: 019db875-56d1-7042-9428-aa02b6b3eecb
Review base: commit 31caf61 (initial commit) → HEAD (2d3dc87)

## Blockers (must fix before 0.1.0 publish)

- [BLOCK-SS-CDX-01] Nonce cache rotation logic bug — valid requests fail after idle periods
  File: src/wssec/nonce_cache.rs:37-39
  Codex finding (P2): The nonce cache only rotates once when `elapsed >= half_window_secs`. After a long idle period (e.g. hours), a nonce older than the full replay window can still remain in `previous` and be rejected as a replay attack. This causes valid UsernameToken requests to fail after low-traffic gaps. Rotation should account for multiple elapsed windows: when at least two full windows have elapsed, clear both buckets.
  Impact: Production correctness issue — legitimate ONVIF clients will get "replay detected" faults on a device that has been idle. Release-blocker.

- [BLOCK-SS-CDX-02] SOAP version inconsistency between envelope serialization and Content-Type header
  File: src/server.rs:612-613
  Codex finding (P2): The response serializes the envelope using `soap_version` derived from `Content-Type` header, but sets the response `Content-Type` from `envelope.soap_version` parsed from the XML envelope namespace. If a client sends mismatched header/envelope versions, the server returns inconsistent responses (SOAP 1.2 envelope with SOAP 1.1 content type or vice versa), breaking interoperability. The fix: either reject mismatches with a `VersionMismatch` fault, or consistently use one validated version throughout.
  Impact: Clients sending mismatched SOAP version signals get malformed responses. ONVIF clients have been observed doing this.

- [BLOCK-SS-CDX-03] xs:choice elements incorrectly treated as required in schema validation
  File: src/dispatch.rs:360-361
  Codex finding (P2): Required-element collection handles `Choice` the same as `Sequence`/`All`. Every `minOccurs>0` child inside an `xs:choice` is treated as mandatory. For ONVIF schemas with choice groups (select exactly one branch), valid requests are rejected before handler invocation with a schema fault.
  Impact: ONVIF operations using `xs:choice` in their schema definitions will always fail validation. This is a correctness blocker.

## Non-blockers (should fix / document known limitations)

- [NB-SS-CDX-01] Docs workflow has duplicate `on.push` YAML keys — tag trigger overrides branch trigger
  File: .github/workflows/docs.yml:4-7
  Codex finding (P3): The workflow defines `on.push` twice. In YAML, the later key overrides the earlier one. As written, pushes to `main` will NOT trigger this workflow — only tag pushes will. This means docs/book deployment won't run on main branch commits.
  Recommendation: Merge both push triggers into one mapping:
  ```yaml
  on:
    push:
      branches: [main]
      tags: ['v*']
    workflow_dispatch:
  ```

- [NB-SS-CDX-02] WSDL address rewriting always uses http:// scheme — breaks HTTPS deployments
  File: src/server.rs:763
  Codex finding (P2, treated as non-blocker for 0.1.0 since docs note HTTPS proxy support as v0.2+): The rewritten `soap:address` is always forced to `http://`. When the service is served via HTTPS (directly or behind a reverse proxy), the `?wsdl` endpoint advertises an incorrect/insecure endpoint and generated SOAP clients target the wrong URL.
  Recommendation: At minimum, document this limitation clearly. Fix: read the `X-Forwarded-Proto` header to determine the scheme, defaulting to `http://` only if not present.

## Codex raw output notes

- Codex reviewed the full diff from initial commit (31caf61) to HEAD (2d3dc87)
- Review transcript: /tmp/codex-soap-run.txt (26788 lines, includes all tool calls)
- Codex ran in read-only sandbox mode; no file modifications made
- Priority labels: P2 = significant issue, P3 = moderate/workflow issue
- Codex also identified the docs.yml YAML key-override issue which Claude missed

## Summary
3 blockers, 2 non-blockers.

Codex P2 findings that are blockers: nonce cache idle-gap bug (CDX-01), SOAP version version mismatch (CDX-02), xs:choice validation false-positive (CDX-03).
Codex P3 findings that are non-blockers: docs.yml YAML duplicate key (CDX-01), WSDL HTTPS scheme (CDX-02).
