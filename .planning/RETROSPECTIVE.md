# Project Retrospective

*A living document updated after each milestone. Lessons feed forward into future planning.*

## Milestone: v1.0 — SOAP Server

**Shipped:** 2026-04-05
**Phases:** 4 | **Plans:** 16 | **Commits:** 71

### What Was Built
- Full WSDL 1.1 parser with two-pass resolution, diamond import dedup, cycle detection
- SOAP 1.1 + 1.2 envelope handling with version-aware Content-Type and fault format
- WS-Security UsernameToken (PasswordDigest + PasswordText) with rotating nonce cache
- Document/literal + RPC/encoded dispatch with O(1) QName routing
- Multi-service WSDL routing with per-service dispatch table isolation
- axum Router integration via ServerBuilder API
- 205 tests (186 unit + 12 integration + 7 ONVIF end-to-end)

### What Worked
- Wave-based parallel execution — plans within a wave run concurrently, cutting wall-clock time significantly
- TDD approach in plans — tests written before implementation caught issues early
- Phase 1 covering 41/47 requirements in one large phase was the right call — the SOAP server subsystems are deeply intertwined and splitting further would have created artificial boundaries
- 3-iteration audit cycle (audit → fix → re-audit) caught real integration gaps that unit tests missed (multi-service WSDL GET, address rewrite)

### What Was Inefficient
- Phase 3 gap closure missed the address rewrite semantic defect, requiring Phase 4 — the integration checker found it on re-audit but it could have been caught in Phase 3 if the checker had run `cargo test` rather than just reading code
- 02-01 executor agent exited early without creating SUMMARY.md — orchestrator had to manually create it, breaking the automation chain
- Some SUMMARY.md files lacked `requirements-completed` frontmatter or had copy-paste errors (02-02, 03-02) — caught by audit but should have been caught by executor self-check

### Patterns Established
- `MatchedPath` extractor for per-route context in shared axum handlers
- `collect_ops_for_service(None, ..)` / `collect_ops_for_service(Some(name), ..)` — single helper drives both global and per-service dispatch table construction
- `SoapServiceRoute` thin wrapper for per-service axum State injection
- Two-bucket `RotatingNonceCache` with `force_rotate()` test helper under `#[cfg(test)]`

### Key Lessons
1. Multi-service mode is a different beast — every handler that touches `svc.mount_path` needs to consider whether it should use the per-service path instead
2. Integration checkers find real bugs that phase verifiers miss — the verifier checks "did the plan execute correctly?" while the checker asks "do the pieces fit together?"
3. Gap closure phases should be planned with integration tests, not just code fixes — Phase 3 fixed the route but not the semantic behavior because no test exercised the full path

### Cost Observations
- Model mix: Opus orchestrated, Sonnet executed all plans and verification, Haiku for quick searches
- 71 commits across 3 calendar days
- Notable: Phase 1 (10 plans, 41 requirements) was the bulk of the work; Phases 2-4 were incremental

---

## Cross-Milestone Trends

### Process Evolution

| Milestone | Commits | Phases | Key Change |
|-----------|---------|--------|------------|
| v1.0 | 71 | 4 | Initial delivery — established audit → gap closure → re-audit cycle |

### Cumulative Quality

| Milestone | Tests | LOC (src) | LOC (tests) |
|-----------|-------|-----------|-------------|
| v1.0 | 205 | 7,825 | 1,314 |

### Top Lessons (Verified Across Milestones)

1. Audit-driven gap closure catches integration issues that per-phase verification misses
2. Multi-service mode needs dedicated integration tests, not just extrapolation from single-service
