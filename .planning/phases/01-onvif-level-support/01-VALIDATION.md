---
phase: 1
slug: onvif-level-support
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-04-04
---

# Phase 1 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust built-in) |
| **Config file** | Cargo.toml |
| **Quick run command** | `cargo test` |
| **Full suite command** | `cargo test -- --include-ignored` |
| **Estimated runtime** | ~15 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test`
- **After every plan wave:** Run `cargo test -- --include-ignored`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 15 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| TBD | TBD | TBD | XSD-01 | unit | `cargo test xsd::` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | XSD-03 | unit | `cargo test xsd::test_extension_chain` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | WSDL-01 | integration | `cargo test wsdl::` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | ENV-01 | unit | `cargo test envelope::` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | FLT-01 | unit | `cargo test fault::` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | DSP-01 | unit | `cargo test dispatch::` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | SEC-02 | unit | `cargo test security::` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | HTTP-01 | integration | `cargo test integration::` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

*Task IDs will be filled by planner after PLAN.md creation.*

---

## Wave 0 Requirements

- [ ] `Cargo.toml` — project skeleton with all dependencies (roxmltree, quick-xml, axum, tokio, sha1, base64, chrono)
- [ ] `tests/fixtures/` — bundled ONVIF WSDLs (devicemgmt.wsdl, onvif.xsd, common.xsd)
- [ ] `src/lib.rs` — crate root with module declarations

*Existing infrastructure: None (greenfield)*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| axum Router composes with other routes | HTTP-01 | Requires running server and external HTTP client | Start server with SOAP + REST routes, curl both endpoints |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 15s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
